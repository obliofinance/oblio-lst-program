use anchor_lang::prelude::*;
use anchor_spl::{
    stake::{Stake, StakeAccount},
    token_interface::{Mint, Token2022, TokenAccount},
};
use solana_stake_interface::state::StakeStateV2;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::DepositStakeEvent;
use crate::math::shares_for_assets;
use crate::state::{Bucket, BucketStatus, GatewayState, MintMode, ProtocolConfig};
use crate::utils::{mint_shares, StakeCpi};

#[derive(Accounts)]
pub struct DepositStake<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        constraint = !gateway.paused @ OblioError::ProtocolPaused
    )]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        mut,
        seeds = [BUCKET_SEED, bucket.vote_account.as_ref()],
        bump = bucket.bump,
        constraint = bucket.status == BucketStatus::Active @ OblioError::InvalidBucketStatus,
        constraint = bucket.is_delegated @ OblioError::BucketNotDelegated
    )]
    pub bucket: Box<Account<'info, Bucket>>,

    /// The user's active, delegated stake account (typed + owner-checked by Anchor).
    #[account(mut)]
    pub user_stake: Box<Account<'info, StakeAccount>>,

    /// CHECK: bucket primary stake PDA (merge destination), pinned to the bucket.
    #[account(mut, address = bucket.stake_account, owner = STAKE_PROGRAM_ID)]
    pub bucket_stake: UncheckedAccount<'info>,

    #[account(mut)]
    pub depositor: Signer<'info>,

    #[account(
        mut,
        token::mint = conf_sol_mint,
        token::authority = depositor,
        token::token_program = token_2022_program,
    )]
    pub recipient_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [MINT_SEED],
        bump = gateway.mint_bump,
        mint::decimals = DECIMALS,
        mint::authority = mint_authority,
        mint::token_program = token_2022_program,
    )]
    pub conf_sol_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: PDA mint authority.
    #[account(seeds = [MINT_AUTHORITY_SEED], bump = gateway.mint_authority_bump)]
    pub mint_authority: UncheckedAccount<'info>,

    pub clock: Sysvar<'info, Clock>,
    /// CHECK: stake history sysvar.
    #[account(address = STAKE_HISTORY_ID)]
    pub stake_history: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub token_2022_program: Program<'info, Token2022>,
    pub stake_program: Program<'info, Stake>,
}

pub fn deposit_stake_handler(ctx: Context<DepositStake>, mode: MintMode, min_shares_out: u64) -> Result<()> {
    let clock = &ctx.accounts.clock;
    let params = ctx.accounts.config.params;
    ctx.accounts.gateway.require_fresh(clock.epoch)?;

    // ---- validate the incoming stake from its typed state ----
    let state: &StakeStateV2 = &***ctx.accounts.user_stake;
    let StakeStateV2::Stake(meta, stake, _) = state else {
        return err!(OblioError::InvalidStakeState);
    };
    require_keys_eq!(stake.delegation.voter_pubkey, ctx.accounts.bucket.vote_account, OblioError::InvalidStakeState);
    require_eq!(stake.delegation.deactivation_epoch, u64::MAX, OblioError::InvalidStakeState);
    require_keys_eq!(meta.authorized.staker, ctx.accounts.depositor.key(), OblioError::Unauthorized);
    require_keys_eq!(meta.authorized.withdrawer, ctx.accounts.depositor.key(), OblioError::Unauthorized);
    require!(!meta.lockup.is_in_force(clock, None), OblioError::StakeLockupInForce);

    let net = ctx.accounts.user_stake.get_lamports();
    require_gte!(net, params.min_deposit_lamports, OblioError::DepositBelowMinimum);

    let gateway = &mut ctx.accounts.gateway;
    let new_total = gateway.total_assets.checked_add(net).ok_or(OblioError::MathOverflow)?;
    require_gte!(params.tvl_cap_lamports, new_total, OblioError::TvlCapExceeded);
    let bucket_after = ctx.accounts.bucket.staked_lamports()?.checked_add(net).ok_or(OblioError::MathOverflow)?;
    let cap = new_total as u128 * params.max_bucket_share_bps as u128 / BPS_DENOMINATOR as u128;
    require_gte!(cap, bucket_after as u128, OblioError::BucketCapExceeded);

    // ---- price ----
    let supply = ctx.accounts.conf_sol_mint.supply;
    let vested = gateway.vested_assets(clock.unix_timestamp)?;
    let shares = shares_for_assets(net, vested, supply, gateway.virtual_offset_lamports)?;
    require_gt!(shares, 0u64, OblioError::ZeroAmount);
    require_gte!(shares, min_shares_out, OblioError::SlippageExceeded);

    // ---- effects ----
    gateway.total_assets = new_total;
    gateway.staked_lamports = gateway.staked_lamports.checked_add(net).ok_or(OblioError::MathOverflow)?;
    ctx.accounts.bucket.active_lamports =
        ctx.accounts.bucket.active_lamports.checked_add(net).ok_or(OblioError::MathOverflow)?;

    // ---- interactions: take authorities, merge atomically into the bucket, mint ----
    let cpi = StakeCpi {
        stake_program: ctx.accounts.stake_program.to_account_info(),
        clock: clock.to_account_info(),
        stake_history: ctx.accounts.stake_history.to_account_info(),
        gateway: gateway.to_account_info(),
        gateway_bump: gateway.bump,
    };
    let user_stake = ctx.accounts.user_stake.to_account_info();
    cpi.take_authorities(&user_stake, &ctx.accounts.depositor.to_account_info())?;
    cpi.merge(&ctx.accounts.bucket_stake.to_account_info(), &user_stake, OblioError::InvalidStakeState)?;

    mint_shares(
        &ctx.accounts.token_2022_program,
        &ctx.accounts.conf_sol_mint,
        &ctx.accounts.recipient_token_account,
        &ctx.accounts.mint_authority,
        ctx.accounts.gateway.mint_authority_bump,
        &ctx.accounts.depositor,
        shares,
        mode,
    )?;

    emit!(DepositStakeEvent {
        depositor: ctx.accounts.depositor.key(),
        stake_account: user_stake.key(),
        bucket: ctx.accounts.bucket.key(),
        lamports: net,
        shares_minted: shares,
        mode,
    });
    Ok(())
}
