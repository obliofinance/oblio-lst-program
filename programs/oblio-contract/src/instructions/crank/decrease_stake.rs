use anchor_lang::prelude::*;
use anchor_spl::stake::Stake;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::StakeDecreasedEvent;
use crate::state::{Bucket, BucketStatus, GatewayState, ProtocolConfig};
use crate::utils::{load_all_buckets, rent_min, StakeCpi};

/// Permissionless unstake pipeline, step 1. `remaining_accounts` = every bucket in index order.
/// The chosen bucket must be the most over-weight one (draining buckets count as target 0),
/// so no caller can steer unstaking onto a validator of their choosing.
#[derive(Accounts)]
pub struct DecreaseStake<'info> {
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        mut,
        seeds = [BUCKET_SEED, bucket.vote_account.as_ref()],
        bump = bucket.bump,
        constraint = bucket.is_delegated @ OblioError::BucketNotDelegated,
        constraint = bucket.unstake_lamports == 0 @ OblioError::UnstakeSlotBusy
    )]
    pub bucket: Box<Account<'info, Bucket>>,

    /// CHECK: primary stake PDA pinned to the bucket.
    #[account(mut, address = bucket.stake_account, owner = STAKE_PROGRAM_ID)]
    pub bucket_stake: UncheckedAccount<'info>,

    /// CHECK: fresh deactivating PDA for the current `unstake_seed` (unused on the in-place path).
    #[account(mut, seeds = [UNSTAKE_SEED, bucket.key().as_ref(), &bucket.unstake_seed.to_le_bytes()], bump)]
    pub unstake_stake: UncheckedAccount<'info>,

    /// Pays the rent reserve of the deactivating account (returns to the pool as a vesting donation).
    #[account(mut)]
    pub payer: Signer<'info>,

    pub clock: Sysvar<'info, Clock>,
    /// CHECK: stake history sysvar.
    #[account(address = STAKE_HISTORY_ID)]
    pub stake_history: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub stake_program: Program<'info, Stake>,
}

pub fn decrease_stake_handler<'info>(ctx: Context<'info, DecreaseStake<'info>>) -> Result<()> {
    let params = ctx.accounts.config.params;
    let gateway_info = ctx.accounts.gateway.to_account_info();
    let physical = gateway_info.lamports();
    let rent = rent_min(&gateway_info)?;
    let gateway = &mut ctx.accounts.gateway;
    let bucket = &mut ctx.accounts.bucket;

    // 1. Use whatever liquidity exists before touching stake.
    gateway.settle_liquidity(physical, rent, params.reserve_ratio_bps)?;

    // 2. Is this the most over-weight bucket?
    let all = load_all_buckets(ctx.remaining_accounts, gateway.bucket_count)?;
    require_keys_eq!(all[bucket.index as usize].key(), bucket.key(), OblioError::InvalidBucketSet);
    let mut total_staked: u128 = 0;
    for b in all.iter() {
        total_staked += b.staked_lamports()? as u128;
    }
    let total_weight = gateway.total_weight as u128;
    let over = |b: &Bucket, staked: u128| -> i128 {
        let target = if total_weight == 0 || b.status == BucketStatus::Draining {
            0
        } else {
            total_staked * b.weight_bps as u128 / total_weight
        };
        staked as i128 - target as i128
    };
    // Only buckets that can actually be decreased take part in the comparison; otherwise one stuck
    // bucket (busy slot / too small to split) would deadlock the whole pipeline.
    let eligible = |b: &Bucket| -> bool {
        b.is_delegated
            && b.unstake_lamports == 0
            && (b.status == BucketStatus::Draining
                || b.active_lamports >= params.min_delegation_lamports.saturating_mul(2))
    };
    let my_staked = bucket.staked_lamports()? as u128;
    let my_over = over(bucket, my_staked);
    require!(eligible(bucket), OblioError::NothingToDecrease);
    for b in all.iter().filter(|b| eligible(b) && b.key() != bucket.key()) {
        require!(over(b, b.staked_lamports()? as u128) <= my_over, OblioError::NotMostOverweight);
    }

    // 3. How much?
    let need = gateway
        .tickets_unfunded()?
        .saturating_sub(gateway.deactivating_lamports);
    let tolerance = total_staked * params.rebalance_tolerance_bps as u128 / BPS_DENOMINATOR as u128;
    let active = bucket.active_lamports;
    let mut amount: u64 = if need > 0 {
        need.min(active)
    } else if bucket.status == BucketStatus::Draining {
        active
    } else if my_over > tolerance as i128 {
        (my_over as u128).min(active as u128) as u64
    } else {
        0
    };
    require!(amount > 0, OblioError::NothingToDecrease);
    amount = amount.max(params.min_delegation_lamports);

    let cpi = StakeCpi {
        stake_program: ctx.accounts.stake_program.to_account_info(),
        clock: ctx.accounts.clock.to_account_info(),
        stake_history: ctx.accounts.stake_history.to_account_info(),
        gateway: gateway_info.clone(),
        gateway_bump: gateway.bump,
    };
    let remaining_after = active.saturating_sub(amount);
    let full_exit = bucket.status == BucketStatus::Draining && remaining_after < params.min_delegation_lamports;

    let in_place;
    if full_exit {
        // Deactivate the whole primary account in place. A transient stake in flight could never be
        // merged into a deactivating primary, so it must be merged first.
        require_eq!(bucket.transient_lamports, 0, OblioError::TransientInFlight);
        in_place = true;
        amount = active;
        cpi.deactivate(&ctx.accounts.bucket_stake.to_account_info())?;
        bucket.is_delegated = false;
        bucket.unstake_in_place = true;
        bucket.unstake_account = bucket.stake_account;
    } else {
        in_place = false;
        // Never leave less than the minimum delegation behind.
        amount = amount.min(active.saturating_sub(params.min_delegation_lamports));
        require_gte!(amount, params.min_delegation_lamports, OblioError::NothingToDecrease);

        let unstake = ctx.accounts.unstake_stake.to_account_info();
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.payer.to_account_info(),
                    to: unstake.clone(),
                },
            ),
            bucket.stake_rent_reserve,
        )?;
        let bucket_key = bucket.key();
        let seed_bytes = bucket.unstake_seed.to_le_bytes();
        let dest_seeds: &[&[u8]] = &[UNSTAKE_SEED, bucket_key.as_ref(), &seed_bytes, &[ctx.bumps.unstake_stake]];
        cpi.split(
            &ctx.accounts.bucket_stake.to_account_info(),
            &unstake,
            &ctx.accounts.system_program.to_account_info(),
            amount,
            dest_seeds,
        )?;
        cpi.deactivate(&unstake)?;
        bucket.unstake_account = unstake.key();
        bucket.unstake_in_place = false;
    }

    bucket.active_lamports = bucket.active_lamports.checked_sub(amount).ok_or(OblioError::MathOverflow)?;
    bucket.unstake_lamports = amount;
    gateway.staked_lamports = gateway.staked_lamports.checked_sub(amount).ok_or(OblioError::MathOverflow)?;
    gateway.deactivating_lamports = gateway.deactivating_lamports.checked_add(amount).ok_or(OblioError::MathOverflow)?;
    emit!(StakeDecreasedEvent { index: bucket.index, lamports: amount, in_place });
    Ok(())
}
