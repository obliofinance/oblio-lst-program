use anchor_lang::prelude::*;
use anchor_spl::stake::Stake;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::BucketDelegatedEvent;
use crate::state::{Bucket, BucketStatus, GatewayState, ProtocolConfig};
use crate::utils::{allocate_stake_pda, initialize_stake, rent_min, StakeCpi};

/// Permissionless. Moves unallocated liquidity towards this bucket's target weight.
/// Call once per bucket (instructions can be batched in one transaction); each call
/// re-derives the target from `staked + free`, which is invariant across sequential calls.
#[derive(Accounts)]
pub struct DelegateToBucket<'info> {
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        mut,
        seeds = [BUCKET_SEED, bucket.vote_account.as_ref()],
        bump = bucket.bump,
        constraint = bucket.status == BucketStatus::Active @ OblioError::InvalidBucketStatus
    )]
    pub bucket: Box<Account<'info, Bucket>>,

    /// CHECK: bucket primary stake PDA (address pinned to the bucket).
    #[account(mut, address = bucket.stake_account)]
    pub bucket_stake: UncheckedAccount<'info>,

    /// CHECK: fresh transient PDA for the current `transient_seed`.
    #[account(mut, seeds = [TRANSIENT_SEED, bucket.key().as_ref(), &bucket.transient_seed.to_le_bytes()], bump)]
    pub transient_stake: UncheckedAccount<'info>,

    /// CHECK: validator vote account pinned to the bucket.
    #[account(address = bucket.vote_account)]
    pub vote_account: UncheckedAccount<'info>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,
    /// CHECK: stake history sysvar.
    #[account(address = STAKE_HISTORY_ID)]
    pub stake_history: UncheckedAccount<'info>,
    /// CHECK: native stake config.
    #[account(address = STAKE_CONFIG_ID)]
    pub stake_config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub stake_program: Program<'info, Stake>,
}

pub fn delegate_to_bucket_handler(ctx: Context<DelegateToBucket>) -> Result<()> {
    let params = ctx.accounts.config.params;
    let gateway_info = ctx.accounts.gateway.to_account_info();
    let physical = gateway_info.lamports();
    let rent = rent_min(&gateway_info)?;
    let gateway = &mut ctx.accounts.gateway;
    let bucket = &mut ctx.accounts.bucket;

    require!(gateway.total_weight > 0 && bucket.weight_bps > 0, OblioError::InvalidWeight);

    let free = gateway.settle_liquidity(physical, rent, params.reserve_ratio_bps)?;
    let basis = gateway.staked_lamports as u128 + free as u128;
    let bps = BPS_DENOMINATOR as u128;
    let target = basis * bucket.weight_bps as u128 / gateway.total_weight as u128;
    let cap = basis * params.max_bucket_share_bps as u128 / bps;
    let want = target.min(cap).saturating_sub(bucket.staked_lamports()? as u128);
    let amount = u64::try_from(want.min(free as u128)).map_err(|_| error!(OblioError::MathOverflow))?;

    let cpi = StakeCpi {
        stake_program: ctx.accounts.stake_program.to_account_info(),
        clock: ctx.accounts.clock.to_account_info(),
        stake_history: ctx.accounts.stake_history.to_account_info(),
        gateway: gateway_info.clone(),
        gateway_bump: gateway.bump,
    };
    let vote = ctx.accounts.vote_account.to_account_info();
    let config_info = ctx.accounts.stake_config.to_account_info();

    if !bucket.is_delegated {
        // (Re-)delegate the primary account: its rent reserve is already inside it.
        require!(bucket.unstake_lamports == 0, OblioError::UnstakeSlotBusy);
        require_gte!(amount, params.min_delegation_lamports, OblioError::InsufficientUnallocatedFunds);
        let stake = ctx.accounts.bucket_stake.to_account_info();
        gateway_info.sub_lamports(amount)?;
        stake.add_lamports(amount)?;
        cpi.delegate(&stake, &vote, &config_info)?;
        bucket.is_delegated = true;
        bucket.active_lamports = bucket.active_lamports.checked_add(amount).ok_or(OblioError::MathOverflow)?;
        emit!(BucketDelegatedEvent { index: bucket.index, lamports: amount, transient: false });
    } else {
        // Top-up through the transient slot (no re-activation of the active stake).
        require!(bucket.transient_lamports == 0, OblioError::TransientSlotBusy);
        let min_amount = params
            .min_delegation_lamports
            .checked_add(bucket.stake_rent_reserve)
            .ok_or(OblioError::MathOverflow)?;
        require_gte!(amount, min_amount, OblioError::InsufficientUnallocatedFunds);

        let transient = ctx.accounts.transient_stake.to_account_info();
        let bucket_key = bucket.key();
        let seed_bytes = bucket.transient_seed.to_le_bytes();
        let bump = ctx.bumps.transient_stake;
        let seeds: &[&[&[u8]]] = &[&[TRANSIENT_SEED, bucket_key.as_ref(), &seed_bytes, &[bump]]];

        gateway_info.sub_lamports(amount)?;
        transient.add_lamports(amount)?;
        allocate_stake_pda(&transient, &ctx.accounts.system_program.to_account_info(), seeds)?;
        initialize_stake(
            &transient,
            &ctx.accounts.rent.to_account_info(),
            &ctx.accounts.stake_program.to_account_info(),
            &gateway.key(),
        )?;
        cpi.delegate(&transient, &vote, &config_info)?;
        bucket.transient_account = transient.key();
        bucket.transient_lamports = amount;
        emit!(BucketDelegatedEvent { index: bucket.index, lamports: amount, transient: true });
    }

    gateway.staked_lamports = gateway.staked_lamports.checked_add(amount).ok_or(OblioError::MathOverflow)?;
    gateway.assert_solvent(gateway_info.lamports(), rent)?;
    Ok(())
}
