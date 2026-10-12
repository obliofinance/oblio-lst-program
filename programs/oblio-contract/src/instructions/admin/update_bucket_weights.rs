use anchor_lang::prelude::*;
use crate::GATEWAY_SEED;
use crate::state::{gateway::GatewayState, bucket::BucketRegistry};
use crate::constants::{BUCKET_COUNT, BUCKET_REGISTRY_SEED};
use crate::errors::OblioError;

#[derive(Accounts)]
pub struct UpdateBucketWeights<'info> {
    #[account(
        has_one = admin,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump
    )]
    pub gateway: Box<Account<'info, GatewayState>>,

    #[account(
        mut,
        seeds = [BUCKET_REGISTRY_SEED],
        bump = registry.bump
    )]
    pub registry: Box<Account<'info, BucketRegistry>>,

    pub admin: Signer<'info>,
}

pub fn update_bucket_weights_handler(
    ctx: Context<UpdateBucketWeights>,
    new_weights_bps: [u16; BUCKET_COUNT],
    whitelist_flags: [bool; BUCKET_COUNT],
) -> Result<()> {
    let mut total_weight: u32 = 0;

    for i in 0..BUCKET_COUNT {
        if whitelist_flags[i] {
            total_weight += new_weights_bps[i] as u32;
        } else {
            require!(new_weights_bps[i] == 0, OblioError::InvalidBucketIndex);
        }
    }

    require!(total_weight == 10_000, OblioError::WeightSumMismatch);

    let registry = &mut ctx.accounts.registry;
    for i in 0..BUCKET_COUNT {
        registry.buckets[i].weight_bps = new_weights_bps[i];
        registry.buckets[i].is_whitelisted = whitelist_flags[i];
    }

    Ok(())
}