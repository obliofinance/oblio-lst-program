use anchor_lang::prelude::*;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::PauseChangedEvent;
use crate::state::{GatewayState, ProtocolConfig};

#[derive(Accounts)]
pub struct Pause<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Account<'info, GatewayState>,
    /// Guardian or admin.
    #[account(constraint = authority.key() == config.guardian || authority.key() == config.admin @ OblioError::Unauthorized)]
    pub authority: Signer<'info>,
}

pub fn pause_handler(ctx: Context<Pause>) -> Result<()> {
    ctx.accounts.gateway.paused = true;
    emit!(PauseChangedEvent { paused: true, by: ctx.accounts.authority.key(), timestamp: Clock::get()?.unix_timestamp });
    Ok(())
}

#[derive(Accounts)]
pub struct Unpause<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Account<'info, GatewayState>,
    pub admin: Signer<'info>,
}

/// Admin-only. Also re-anchors the circuit-breaker rate bounds on the next sync.
pub fn unpause_handler(ctx: Context<Unpause>) -> Result<()> {
    let g = &mut ctx.accounts.gateway;
    g.paused = false;
    g.breaker_tripped = false;
    g.last_rate_wad = 0;
    emit!(PauseChangedEvent { paused: false, by: ctx.accounts.admin.key(), timestamp: Clock::get()?.unix_timestamp });
    Ok(())
}

use crate::events::BucketDrainedEvent;
use crate::state::{Bucket, BucketStatus};

/// Safety direction only: guardian (or admin) may instantly stop feeding a misbehaving validator.
/// Re-activating or re-weighting still requires the timelock.
#[derive(Accounts)]
pub struct DrainBucket<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Account<'info, GatewayState>,
    #[account(mut, seeds = [BUCKET_SEED, bucket.vote_account.as_ref()], bump = bucket.bump)]
    pub bucket: Account<'info, Bucket>,
    #[account(constraint = authority.key() == config.guardian || authority.key() == config.admin @ OblioError::Unauthorized)]
    pub authority: Signer<'info>,
}

pub fn drain_bucket_handler(ctx: Context<DrainBucket>) -> Result<()> {
    let b = &mut ctx.accounts.bucket;
    let g = &mut ctx.accounts.gateway;
    g.total_weight = g.total_weight.checked_sub(b.weight_bps as u32).ok_or(OblioError::MathOverflow)?;
    b.weight_bps = 0;
    b.status = BucketStatus::Draining;
    emit!(BucketDrainedEvent { index: b.index, by: ctx.accounts.authority.key() });
    Ok(())
}
