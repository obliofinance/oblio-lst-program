use anchor_lang::{prelude::*, solana_program};
use solana_stake_interface::instruction::deactivate_stake;
use crate::state::{gateway::GatewayState, bucket::BucketRegistry};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::StakeDeactivatedEvent;

#[derive(Accounts)]
#[instruction(bucket_index: u8)]
pub struct DeactivateBucketStake<'info> {
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
    pub admin: Signer<'info>,
    /// CHECK: Native Stake Program
    #[account(address = solana_stake_interface::program::ID @ OblioError::InvalidStakeProgram)]
    pub stake_program: UncheckedAccount<'info>,
}

pub fn deactivate_bucket_stake_handler(
    ctx: Context<DeactivateBucketStake>,
    bucket_index: u8,
) -> Result<()> {
    require!((bucket_index as usize) < BUCKET_COUNT, OblioError::InvalidBucketIndex);
    let bucket = &ctx.accounts.registry.buckets[bucket_index as usize];
    require_keys_eq!(bucket.stake_account, ctx.accounts.stake_account.key());

    let gateway = &ctx.accounts.gateway;
    let gateway_seeds: &[&[&[u8]]] = &[&[GATEWAY_SEED, &[gateway.bump]]];

    let ix_deactivate = deactivate_stake(&bucket.stake_account, &gateway.key());

    solana_program::program::invoke_signed(
        &ix_deactivate,
        &[
            ctx.accounts.stake_account.to_account_info(),
            ctx.accounts.clock.to_account_info(),
            gateway.to_account_info(),
            ctx.accounts.stake_program.to_account_info(),
        ],
        gateway_seeds,
    )?;

    emit!(StakeDeactivatedEvent {
        bucket_index,
        stake_account: bucket.stake_account,
    });

    Ok(())
}