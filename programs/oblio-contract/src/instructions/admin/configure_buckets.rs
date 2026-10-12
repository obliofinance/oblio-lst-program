use anchor_lang::prelude::*;
use crate::state::{gateway::GatewayState, bucket::{BucketRegistry, BucketInfo}};
use crate::constants::*;

#[derive(Accounts)]
pub struct ConfigureBuckets<'info> {
    #[account(
        mut, 
        has_one = admin,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump
    )]
    pub gateway: Box<Account<'info, GatewayState>>,

    #[account(
        init,
        payer = admin,
        space = BucketRegistry::LEN,
        seeds = [BUCKET_REGISTRY_SEED],
        bump
    )]
    pub registry: Box<Account<'info, BucketRegistry>>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn configure_buckets_handler(
    ctx: Context<ConfigureBuckets>,
    vote_accounts: [Pubkey; BUCKET_COUNT],
) -> Result<()> {
    let registry = &mut ctx.accounts.registry;
    registry.total_delegated_lamports = 0;
    registry.bump = ctx.bumps.registry;
    registry.buckets = Vec::with_capacity(BUCKET_COUNT);

    for i in 0..BUCKET_COUNT {
        registry.buckets.push(BucketInfo {
            stake_account: Pubkey::default(),
            transient_account: Pubkey::default(),
            vote_account: vote_accounts[i],
            weight_bps: 0,
            active_lamports: 0,
            transient_lamports: 0,
            transient_seed: 0,
            is_whitelisted: false,
            bump: 0,
            transient_bump: 0,
        });
    }

    Ok(())
}