use anchor_lang::prelude::*;
use anchor_spl::stake::Stake;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::{InactiveStakeWithdrawnEvent, LiquiditySettledEvent};
use crate::state::{Bucket, GatewayState, ProtocolConfig};
use crate::utils::{rent_min, StakeCpi};

/// Permissionless unstake pipeline, step 2: pull a fully inactive stake account back into the
/// gateway and immediately route it (ticket queue -> reserve -> free).
#[derive(Accounts)]
pub struct WithdrawInactiveStake<'info> {
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        mut,
        seeds = [BUCKET_SEED, bucket.vote_account.as_ref()],
        bump = bucket.bump,
        constraint = bucket.unstake_lamports > 0 @ OblioError::NothingToClaim
    )]
    pub bucket: Box<Account<'info, Bucket>>,

    /// CHECK: deactivating account pinned to the bucket (split PDA, or the primary when in place).
    #[account(mut, address = bucket.unstake_account, owner = STAKE_PROGRAM_ID)]
    pub unstake_stake: UncheckedAccount<'info>,

    pub clock: Sysvar<'info, Clock>,
    /// CHECK: stake history sysvar.
    #[account(address = STAKE_HISTORY_ID)]
    pub stake_history: UncheckedAccount<'info>,
    pub stake_program: Program<'info, Stake>,
}

pub fn withdraw_inactive_stake_handler(ctx: Context<WithdrawInactiveStake>) -> Result<()> {
    let params = ctx.accounts.config.params;
    let gateway_info = ctx.accounts.gateway.to_account_info();
    let rent = rent_min(&gateway_info)?;
    let gateway = &mut ctx.accounts.gateway;
    let bucket = &mut ctx.accounts.bucket;

    let total = ctx.accounts.unstake_stake.get_lamports();
    // In place the primary account stays alive and keeps its rent reserve.
    let amount = if bucket.unstake_in_place {
        total.checked_sub(bucket.stake_rent_reserve).ok_or(OblioError::MathOverflow)?
    } else {
        total
    };
    require_gt!(amount, 0u64, OblioError::NothingToClaim);

    let cpi = StakeCpi {
        stake_program: ctx.accounts.stake_program.to_account_info(),
        clock: ctx.accounts.clock.to_account_info(),
        stake_history: ctx.accounts.stake_history.to_account_info(),
        gateway: gateway_info.clone(),
        gateway_bump: gateway.bump,
    };
    cpi.withdraw_to_gateway(&ctx.accounts.unstake_stake.to_account_info(), amount)?;

    gateway.deactivating_lamports = gateway
        .deactivating_lamports
        .checked_sub(bucket.unstake_lamports)
        .ok_or(OblioError::MathOverflow)?;
    if !bucket.unstake_in_place {
        bucket.unstake_seed = bucket.unstake_seed.checked_add(1).ok_or(OblioError::MathOverflow)?;
    }
    emit!(InactiveStakeWithdrawnEvent { index: bucket.index, lamports: amount });
    bucket.unstake_lamports = 0;
    bucket.unstake_account = Pubkey::default();
    bucket.unstake_in_place = false;

    let free = gateway.settle_liquidity(gateway_info.lamports(), rent, params.reserve_ratio_bps)?;
    gateway.assert_solvent(gateway_info.lamports(), rent)?;
    emit!(LiquiditySettledEvent { tickets_funded: gateway.ticket_queue_funded, reserve: gateway.reserve_pool_lamports, free });
    Ok(())
}
