use anchor_lang::{prelude::*, solana_program};
use solana_stake_interface::instruction::withdraw;
use crate::state::{gateway::GatewayState, bucket::BucketRegistry};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::StakeWithdrawnEvent;

#[derive(Accounts)]
#[instruction(bucket_index: u8)]
pub struct WithdrawBucketStake<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        has_one = admin @ OblioError::Unauthorized
    )]
    pub gateway: Box<Account<'info, GatewayState>>,

    #[account(
        mut,
        seeds = [BUCKET_REGISTRY_SEED],
        bump = registry.bump
    )]
    pub registry: Box<Account<'info, BucketRegistry>>,

    /// CHECK: Validated bucket stake account
    #[account(mut)]
    pub stake_account: UncheckedAccount<'info>,

    pub clock: Sysvar<'info, Clock>,
    /// CHECK: Stake History Sysvar
    #[account(address = solana_stake_interface::sysvar::stake_history::ID @ OblioError::InvalidSysvar)]
    pub stake_history: UncheckedAccount<'info>,

    pub admin: Signer<'info>,
    /// CHECK: Native Stake Program
    #[account(address = solana_stake_interface::program::ID @ OblioError::InvalidStakeProgram)]
    pub stake_program: UncheckedAccount<'info>,
}

pub fn withdraw_bucket_stake_handler(
    ctx: Context<WithdrawBucketStake>,
    bucket_index: u8,
    lamports_to_withdraw: u64,
) -> Result<()> {
    require!((bucket_index as usize) < BUCKET_COUNT, OblioError::InvalidBucketIndex);
    require!(lamports_to_withdraw > 0, OblioError::MathOverflow);

    let idx = bucket_index as usize;
    let stake_account_key = ctx.accounts.registry.buckets[idx].stake_account;
    require_keys_eq!(stake_account_key, ctx.accounts.stake_account.key());

    let gateway = &ctx.accounts.gateway;
    let gateway_seeds: &[&[&[u8]]] = &[&[GATEWAY_SEED, &[gateway.bump]]];

    let ix_withdraw = withdraw(
        &stake_account_key,
        &gateway.key(),
        &gateway.key(),
        lamports_to_withdraw,
        None,
    );

    solana_program::program::invoke_signed(
        &ix_withdraw,
        &[
            ctx.accounts.stake_account.to_account_info(),
            gateway.to_account_info(),
            ctx.accounts.clock.to_account_info(),
            ctx.accounts.stake_history.to_account_info(),
            gateway.to_account_info(),
            ctx.accounts.stake_program.to_account_info(),
        ],
        gateway_seeds,
    )?;

    let registry = &mut ctx.accounts.registry;
    registry.buckets[idx].active_lamports = registry.buckets[idx]
        .active_lamports
        .saturating_sub(lamports_to_withdraw);
    registry.total_delegated_lamports = registry
        .total_delegated_lamports
        .saturating_sub(lamports_to_withdraw);

    let gateway_state = &mut ctx.accounts.gateway;
    let target = gateway_state.target_reserve_lamports;
    let current = gateway_state.reserve_pool_lamports;
    if current < target {
        let deficit = target.saturating_sub(current);
        let refill = deficit.min(lamports_to_withdraw);
        gateway_state.reserve_pool_lamports = gateway_state.reserve_pool_lamports.saturating_add(refill);
    }

    emit!(StakeWithdrawnEvent {
        bucket_index,
        stake_account: stake_account_key,
        lamports_withdrawn: lamports_to_withdraw,
    });

    Ok(())
}