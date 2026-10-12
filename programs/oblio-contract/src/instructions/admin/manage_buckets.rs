use anchor_lang::prelude::*;
use anchor_spl::stake::Stake;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::*;
use crate::state::{Bucket, BucketStatus, GatewayState, ProtocolConfig, TimelockedAction};
use crate::utils::{allocate_stake_pda, initialize_stake, stake_rent_reserve};

#[derive(Accounts)]
pub struct RegisterBucket<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Box<Account<'info, ProtocolConfig>>,

    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Box<Account<'info, GatewayState>>,

    #[account(
        init,
        payer = admin,
        space = 8 + Bucket::INIT_SPACE,
        seeds = [BUCKET_SEED, vote_account.key().as_ref()],
        bump
    )]
    pub bucket: Box<Account<'info, Bucket>>,

    /// CHECK: bucket primary stake PDA, set up in the handler.
    #[account(mut, seeds = [STAKE_SEED, bucket.key().as_ref()], bump)]
    pub stake_account: UncheckedAccount<'info>,

    /// CHECK: must be owned by the vote program.
    #[account(owner = VOTE_PROGRAM_ID)]
    pub vote_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub stake_program: Program<'info, Stake>,
}

pub fn register_bucket_handler(ctx: Context<RegisterBucket>) -> Result<()> {
    let gateway = &mut ctx.accounts.gateway;
    require_gt!(ctx.accounts.config.params.max_buckets, gateway.bucket_count, OblioError::MaxBucketsReached);

    let reserve = stake_rent_reserve()?;
    let stake_bump = ctx.bumps.stake_account;
    let bucket_key = ctx.accounts.bucket.key();
    let seeds: &[&[&[u8]]] = &[&[STAKE_SEED, bucket_key.as_ref(), &[stake_bump]]];

    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.key(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.admin.to_account_info(),
                to: ctx.accounts.stake_account.to_account_info(),
            },
        ),
        reserve,
    )?;
    allocate_stake_pda(
        &ctx.accounts.stake_account.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        seeds,
    )?;
    initialize_stake(
        &ctx.accounts.stake_account.to_account_info(),
        &ctx.accounts.rent.to_account_info(),
        &ctx.accounts.stake_program.to_account_info(),
        &gateway.key(),
    )?;

    let b = &mut ctx.accounts.bucket;
    b.index = gateway.bucket_count;
    b.vote_account = ctx.accounts.vote_account.key();
    b.stake_account = ctx.accounts.stake_account.key();
    b.transient_account = Pubkey::default();
    b.unstake_account = Pubkey::default();
    b.status = BucketStatus::Active;
    b.weight_bps = 0;
    b.is_delegated = false;
    b.unstake_in_place = false;
    b.stake_rent_reserve = reserve;
    b.bump = ctx.bumps.bucket;
    b.stake_bump = stake_bump;

    gateway.bucket_count = gateway.bucket_count.checked_add(1).ok_or(OblioError::MathOverflow)?;
    emit!(BucketRegisteredEvent { index: b.index, bucket: bucket_key, vote_account: b.vote_account });
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateBucket<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Account<'info, GatewayState>,
    #[account(mut, seeds = [BUCKET_SEED, bucket.vote_account.as_ref()], bump = bucket.bump)]
    pub bucket: Account<'info, Bucket>,
    pub admin: Signer<'info>,
}

pub fn update_bucket_handler(ctx: Context<UpdateBucket>, weight_bps: u16, status: BucketStatus) -> Result<()> {
    let b = &mut ctx.accounts.bucket;
    ctx.accounts.config.consume_action(
        Clock::get()?.unix_timestamp,
        &TimelockedAction::UpdateBucket { bucket: b.key(), weight_bps, status },
    )?;
    match status {
        BucketStatus::Draining => require_eq!(weight_bps, 0, OblioError::InvalidWeight),
        BucketStatus::Active => {
            // A bucket whose primary is being deactivated cannot be re-activated until withdrawn.
            require!(!b.unstake_in_place || b.unstake_lamports == 0, OblioError::UnstakeSlotBusy);
            require_gte!(10_000u16, weight_bps, OblioError::InvalidWeight);
        }
    }
    let g = &mut ctx.accounts.gateway;
    g.total_weight = g
        .total_weight
        .checked_sub(b.weight_bps as u32)
        .and_then(|v| v.checked_add(weight_bps as u32))
        .ok_or(OblioError::MathOverflow)?;
    b.weight_bps = weight_bps;
    b.status = status;
    emit!(BucketUpdatedEvent { index: b.index, weight_bps, status });
    Ok(())
}
