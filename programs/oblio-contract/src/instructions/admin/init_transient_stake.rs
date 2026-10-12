use anchor_lang::{prelude::*, solana_program};
use crate::state::{gateway::GatewayState, bucket::BucketRegistry};
use crate::constants::*;
use crate::errors::OblioError;

#[derive(Accounts)]
#[instruction(bucket_index: u8)]
pub struct InitTransientStake<'info> {
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

    /// CHECK: Transient Stake Account PDA for non-churn warmup top-ups
    #[account(
        mut,
        seeds = [TRANSIENT_STAKE_SEED, &[bucket_index]],
        bump
    )]
    pub transient_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    /// CHECK: Native Stake Program ID
    #[account(address = solana_stake_interface::program::ID @ OblioError::InvalidStakeProgram)]
    pub stake_program: UncheckedAccount<'info>,
}

pub fn init_transient_stake_handler(ctx: Context<InitTransientStake>, bucket_index: u8) -> Result<()> {
    require!((bucket_index as usize) < BUCKET_COUNT, OblioError::InvalidBucketIndex);

    let rent = &ctx.accounts.rent;
    let stake_space = std::mem::size_of::<solana_stake_interface::state::StakeStateV2>();
    let stake_rent = rent.minimum_balance(stake_space);

    let (transient_pda, bump) = Pubkey::find_program_address(&[TRANSIENT_STAKE_SEED, &[bucket_index]], ctx.program_id);
    require_keys_eq!(transient_pda, ctx.accounts.transient_account.key());

    let ix_create = anchor_lang::solana_program::system_instruction::create_account(
        &ctx.accounts.admin.key(),
        &transient_pda,
        stake_rent,
        stake_space as u64,
        &solana_stake_interface::program::ID,
    );

    let signer_seeds: &[&[&[u8]]] = &[&[TRANSIENT_STAKE_SEED, &[bucket_index], &[bump]]];

    solana_program::program::invoke_signed(
        &ix_create,
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.transient_account.to_account_info(),
        ],
        signer_seeds,
    )?;

    let authorized = solana_stake_interface::state::Authorized {
        staker: ctx.accounts.gateway.key(),
        withdrawer: ctx.accounts.gateway.key(),
    };

    let ix_init_stake = solana_stake_interface::instruction::initialize(
        &transient_pda,
        &authorized,
        &solana_stake_interface::state::Lockup::default(),
    );

    solana_program::program::invoke(
        &ix_init_stake,
        &[
            ctx.accounts.transient_account.to_account_info(),
            ctx.accounts.rent.to_account_info(),
            ctx.accounts.stake_program.to_account_info(),
        ],
    )?;

    let registry = &mut ctx.accounts.registry;
    registry.buckets[bucket_index as usize].transient_account = transient_pda;
    registry.buckets[bucket_index as usize].transient_bump = bump;

    Ok(())
}