use anchor_lang::prelude::*;
use anchor_spl::stake::Stake;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::TransientMergedEvent;
use crate::state::{Bucket, GatewayState};
use crate::utils::StakeCpi;

#[derive(Accounts)]
pub struct MergeTransientStake<'info> {
    #[account(seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Box<Account<'info, GatewayState>>,

    #[account(
        mut,
        seeds = [BUCKET_SEED, bucket.vote_account.as_ref()],
        bump = bucket.bump,
        constraint = bucket.transient_lamports > 0 @ OblioError::TransientStakeNotActive
    )]
    pub bucket: Box<Account<'info, Bucket>>,

    /// CHECK: merge destination, pinned to the bucket.
    #[account(mut, address = bucket.stake_account, owner = STAKE_PROGRAM_ID)]
    pub bucket_stake: UncheckedAccount<'info>,

    /// CHECK: merge source (drained and closed by the stake program), pinned to the bucket.
    #[account(mut, address = bucket.transient_account, owner = STAKE_PROGRAM_ID)]
    pub transient_stake: UncheckedAccount<'info>,

    pub clock: Sysvar<'info, Clock>,
    /// CHECK: stake history sysvar.
    #[account(address = STAKE_HISTORY_ID)]
    pub stake_history: UncheckedAccount<'info>,
    pub stake_program: Program<'info, Stake>,
}

pub fn merge_transient_stake_handler(ctx: Context<MergeTransientStake>) -> Result<()> {
    let cpi = StakeCpi {
        stake_program: ctx.accounts.stake_program.to_account_info(),
        clock: ctx.accounts.clock.to_account_info(),
        stake_history: ctx.accounts.stake_history.to_account_info(),
        gateway: ctx.accounts.gateway.to_account_info(),
        gateway_bump: ctx.accounts.gateway.bump,
    };
    cpi.merge(
        &ctx.accounts.bucket_stake.to_account_info(),
        &ctx.accounts.transient_stake.to_account_info(),
        OblioError::TransientStakeNotActive,
    )?;

    let b = &mut ctx.accounts.bucket;
    let merged = b.transient_lamports;
    // All transient lamports (incl. its rent) become primary stake above the primary's own reserve.
    b.active_lamports = b.active_lamports.checked_add(merged).ok_or(OblioError::MathOverflow)?;
    b.transient_lamports = 0;
    b.transient_account = Pubkey::default();
    b.transient_seed = b.transient_seed.checked_add(1).ok_or(OblioError::MathOverflow)?;
    emit!(TransientMergedEvent { index: b.index, lamports: merged });
    Ok(())
}
