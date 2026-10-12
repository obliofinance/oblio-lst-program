use anchor_lang::{
    prelude::*,
    solana_program::program::invoke_signed,
};
use solana_epoch_rewards::EpochRewards;
use anchor_spl::{
    token_2022::spl_token_2022::extension::scaled_ui_amount::instruction::update_multiplier,
    token_interface::{Mint, Token2022},
};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::*;
use crate::state::{Bucket, GatewayState, ProtocolConfig};
use crate::utils::rent_min;

/// Permissionless. `remaining_accounts` = for every bucket, in index order:
/// `[bucket (mut), primary stake, transient stake, unstake stake]`.
/// Transient/unstake entries are only read when the bucket's counters say the slot is in use
/// (pass any account otherwise, e.g. the program id).
#[derive(Accounts)]
pub struct SyncRate<'info> {
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,
    #[account(mut, seeds = [MINT_SEED], bump = gateway.mint_bump, mint::token_program = token_2022_program)]
    pub conf_sol_mint: Box<InterfaceAccount<'info, Mint>>,
    /// CHECK: PDA scaled-UI authority.
    #[account(seeds = [MINT_AUTHORITY_SEED], bump = gateway.mint_authority_bump)]
    pub mint_authority: UncheckedAccount<'info>,
    pub token_2022_program: Program<'info, Token2022>,
}

fn trip(gateway: &mut GatewayState, reason: u8, old: u64, new: u64, real: u64) {
    gateway.paused = true;
    gateway.breaker_tripped = true;
    emit!(CircuitBreakerTrippedEvent {
        reason,
        old_rate_wad: old,
        new_rate_wad: new,
        total_assets: gateway.total_assets,
        real_assets: real,
    });
}

pub fn sync_rate_handler<'info>(ctx: Context<'info, SyncRate<'info>>) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    // Rewards are credited in partitions at the start of an epoch; never price against a half-distributed epoch.
    require!(!<EpochRewards as solana_get_sysvar::GetSysvar>::get()?.active, OblioError::EpochRewardsActive);

    let params = ctx.accounts.config.params;
    let supply = ctx.accounts.conf_sol_mint.supply;
    let physical = ctx.accounts.gateway.get_lamports();
    let rent = rent_min(&ctx.accounts.gateway.to_account_info())?;
    let mint_authority_bump = ctx.accounts.gateway.mint_authority_bump;
    let gateway: &mut GatewayState = &mut ctx.accounts.gateway;

    require_eq!(
        ctx.remaining_accounts.len(),
        gateway.bucket_count as usize * BUCKET_ACCOUNTS_PER_SYNC,
        OblioError::InvalidBucketSet
    );

    // ---- 1. read the real value of every stake account ----
    let (mut value_total, mut delegated, mut in_flight) = (0u64, 0u64, 0u64);
    for (i, chunk) in ctx.remaining_accounts.chunks_exact(BUCKET_ACCOUNTS_PER_SYNC).enumerate() {
        let mut bucket = Account::<Bucket>::try_from(&chunk[0])?;
        require_eq!(bucket.index as usize, i, OblioError::InvalidBucketSet);
        require!(chunk[0].is_writable, OblioError::InvalidBucketSet);

        let stake = &chunk[1];
        require_keys_eq!(stake.key(), bucket.stake_account, OblioError::InvalidBucketSet);
        require_keys_eq!(*stake.owner, STAKE_PROGRAM_ID, OblioError::InvalidStakeState);
        let primary_net = stake
            .lamports()
            .checked_sub(bucket.stake_rent_reserve)
            .ok_or(OblioError::MathOverflow)?;
        value_total = value_total.checked_add(primary_net).ok_or(OblioError::MathOverflow)?;

        if bucket.unstake_in_place {
            bucket.active_lamports = 0;
            bucket.unstake_lamports = primary_net;
        } else {
            bucket.active_lamports = primary_net;
        }

        if bucket.transient_lamports > 0 {
            let t = &chunk[2];
            require_keys_eq!(t.key(), bucket.transient_account, OblioError::InvalidBucketSet);
            require_keys_eq!(*t.owner, STAKE_PROGRAM_ID, OblioError::InvalidStakeState);
            // Transient rent is NOT subtracted: it is absorbed into the primary on merge.
            bucket.transient_lamports = t.lamports();
            value_total = value_total.checked_add(t.lamports()).ok_or(OblioError::MathOverflow)?;
        }

        if bucket.unstake_lamports > 0 && !bucket.unstake_in_place {
            let u = &chunk[3];
            require_keys_eq!(u.key(), bucket.unstake_account, OblioError::InvalidBucketSet);
            require_keys_eq!(*u.owner, STAKE_PROGRAM_ID, OblioError::InvalidStakeState);
            bucket.unstake_lamports = u.lamports().saturating_sub(bucket.stake_rent_reserve);
            value_total = value_total.checked_add(u.lamports()).ok_or(OblioError::MathOverflow)?;
        }

        delegated = delegated.checked_add(bucket.staked_lamports()?).ok_or(OblioError::MathOverflow)?;
        in_flight = in_flight.checked_add(bucket.unstake_lamports).ok_or(OblioError::MathOverflow)?;
        bucket.exit(&crate::ID)?;
    }
    gateway.staked_lamports = delegated;
    gateway.deactivating_lamports = in_flight;

    // ---- 2. real assets backing shares ----
    let non_share = gateway
        .protocol_fee_lamports
        .checked_add(gateway.tickets_outstanding()?)
        .ok_or(OblioError::MathOverflow)?;
    let real = value_total
        .checked_add(physical)
        .and_then(|v| v.checked_sub(rent))
        .and_then(|v| v.checked_sub(non_share))
        .ok_or(OblioError::Insolvent)?;

    // ---- 3. candidate state (applied only if the circuit breaker stays closed) ----
    let mut cand: GatewayState = gateway.clone();
    let mut profit = 0u64;
    if real >= cand.total_assets {
        profit = real - cand.total_assets;
        cand.total_assets = real;
        cand.lock_profit(profit, now, params.profit_vesting_secs)?;
    } else {
        let loss = cand.total_assets - real;
        let tolerance = (cand.total_assets as u128 * params.rate_drop_tolerance_bps as u128
            / BPS_DENOMINATOR as u128) as u64;
        if loss > tolerance {
            let old = gateway.last_rate_wad;
            trip(gateway, 1, old, 0, real);
            return Ok(());
        }
        cand.absorb_loss(loss, now);
        cand.total_assets = real;
    }

    // ---- 4. rate + bounds ----
    let (num, den) = cand.rate_parts(supply, now)?;
    let rate_wad = u64::try_from(num.checked_mul(WAD).ok_or(OblioError::MathOverflow)? / den)
        .map_err(|_| error!(OblioError::MathOverflow))?;
    // Downward moves beyond tolerance trip the breaker. Upward moves are *clamped* (display multiplier only):
    // settlement always uses the exact vested rate, so a donation or a long gap can never be turned into a pause.
    let mut pushed_rate_wad = rate_wad;
    if gateway.last_rate_wad != 0 {
        let last = gateway.last_rate_wad as u128;
        let bps = BPS_DENOMINATOR as u128;
        let lo = last * (bps - params.rate_drop_tolerance_bps as u128) / bps;
        let hi = last * (bps + params.max_rate_jump_bps as u128) / bps;
        if (rate_wad as u128) < lo {
            let old = gateway.last_rate_wad;
            trip(gateway, 2, old, rate_wad, real);
            return Ok(());
        }
        pushed_rate_wad = rate_wad.min(hi as u64);
    }
    let multiplier = (pushed_rate_wad as f64) / (WAD as f64);
    require!(multiplier.is_finite() && multiplier > 0.0, OblioError::MathOverflow);

    gateway.total_assets = cand.total_assets;
    gateway.locked_profit = cand.locked_profit;
    gateway.lock_start_ts = cand.lock_start_ts;
    gateway.lock_duration_secs = cand.lock_duration_secs;
    gateway.last_synced_epoch = clock.epoch;
    gateway.last_rate_wad = pushed_rate_wad;

    // ---- 5. push the multiplier to Token-2022 ----
    let ix = update_multiplier(
        &ctx.accounts.token_2022_program.key(),
        &ctx.accounts.conf_sol_mint.key(),
        &ctx.accounts.mint_authority.key(),
        &[],
        multiplier,
        now,
    )?;
    let seeds: &[&[&[u8]]] = &[&[MINT_AUTHORITY_SEED, &[mint_authority_bump]]];
    invoke_signed(
        &ix,
        &[
            ctx.accounts.conf_sol_mint.to_account_info(),
            ctx.accounts.mint_authority.to_account_info(),
            ctx.accounts.token_2022_program.to_account_info(),
        ],
        seeds,
    )?;

    emit!(RateSyncedEvent {
        epoch: clock.epoch,
        total_assets: gateway.total_assets,
        locked_profit: gateway.locked_profit_at(now),
        supply,
        rate_wad: pushed_rate_wad,
        multiplier,
        profit,
    });
    Ok(())
}
