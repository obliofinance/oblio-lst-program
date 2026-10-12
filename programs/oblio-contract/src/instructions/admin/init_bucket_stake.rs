use anchor_lang::{prelude::*, solana_program};
use crate::state::{gateway::GatewayState, bucket::BucketRegistry};
use crate::constants::*;
use crate::errors::OblioError;

#[derive(Accounts)]
#[instruction(bucket_index: u8)]
pub struct InitBucketStake<'info> {
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

    /// CHECK: Primary Stake Account PDA
    #[account(
        mut,
        seeds = [STAKE_BUCKET_SEED, &[bucket_index]],
        bump
    )]
    pub stake_account: UncheckedAccount<'info>,

    /// CHECK: Vote account to associate with this bucket
    pub vote_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    /// CHECK: Native Stake Program ID
    #[account(address = solana_stake_interface::program::ID @ OblioError::InvalidStakeProgram)]
    pub stake_program: UncheckedAccount<'info>,
}

pub fn init_bucket_stake_handler(ctx: Context<InitBucketStake>, bucket_index: u8) -> Result<()> {
    require!((bucket_index as usize) < BUCKET_COUNT, OblioError::InvalidBucketIndex);

    let rent = &ctx.accounts.rent;
    let stake_space = std::mem::size_of::<solana_stake_interface::state::StakeStateV2>();
    let stake_rent = rent.minimum_balance(stake_space);

    let (stake_pda, bump) = Pubkey::find_program_address(&[STAKE_BUCKET_SEED, &[bucket_index]], ctx.program_id);
    require_keys_eq!(stake_pda, ctx.accounts.stake_account.key());

    let ix_create = anchor_lang::solana_program::system_instruction::create_account(
        &ctx.accounts.admin.key(),
        &stake_pda,
        stake_rent,
        stake_space as u64,
        &solana_stake_interface::program::ID,
    );

    let signer_seeds: &[&[&[u8]]] = &[&[STAKE_BUCKET_SEED, &[bucket_index], &[bump]]];

    solana_program::program::invoke_signed(
        &ix_create,
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.stake_account.to_account_info(),
        ],
        signer_seeds,
    )?;

    let authorized = solana_stake_interface::state::Authorized {
        staker: ctx.accounts.gateway.key(),
        withdrawer: ctx.accounts.gateway.key(),
    };

    let ix_init_stake = solana_stake_interface::instruction::initialize(
        &stake_pda,
        &authorized,
        &solana_stake_interface::state::Lockup::default(),
    );

    solana_program::program::invoke(
        &ix_init_stake,
        &[
            ctx.accounts.stake_account.to_account_info(),
            ctx.accounts.rent.to_account_info(),
            ctx.accounts.stake_program.to_account_info(),
        ],
    )?;

    let registry = &mut ctx.accounts.registry;
    registry.buckets[bucket_index as usize].stake_account = stake_pda;
    registry.buckets[bucket_index as usize].vote_account = ctx.accounts.vote_account.key();
    registry.buckets[bucket_index as usize].bump = bump;

    Ok(())
}