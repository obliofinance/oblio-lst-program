use anchor_lang::{prelude::*, solana_program};
use anchor_spl::token_2022::spl_token_2022::extension::scaled_ui_amount::instruction::update_multiplier;
use crate::state::{gateway::GatewayState, bucket::BucketRegistry};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::CrankRateUpdatedEvent;

#[derive(Accounts)]
pub struct UpdateCrankMultiplier<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump
    )]
    pub gateway: Box<Account<'info, GatewayState>>,
    pub registry: Box<Account<'info, BucketRegistry>>,

    /// CHECK: Confirmed Token-2022 mint
    #[account(mut, address = gateway.conf_sol_mint @ OblioError::InvalidMint)]
    pub conf_sol_mint: UncheckedAccount<'info>,

    /// CHECK: PDA signer for mint authority
    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump = gateway.mint_authority_bump
    )]
    pub mint_authority: UncheckedAccount<'info>,

    /// CHECK: Token-2022 Program ID
    #[account(address = anchor_spl::token_2022::ID)]
    pub token_2022_program: UncheckedAccount<'info>,

    /// Anyone can permissionlessly execute this maintenance crank
    pub crank_signer: Signer<'info>,
}

pub fn update_crank_multiplier_handler<'info>(
    ctx: Context<'info, UpdateCrankMultiplier<'info>>,
) -> Result<()> {
    let gateway = &mut ctx.accounts.gateway;
    let clock = Clock::get()?;

    let gateway_info = gateway.to_account_info();
    let min_rent = Rent::get()?.minimum_balance(gateway_info.data_len());
    let gateway_lamports = gateway_info.lamports();

    let physical_liquid = gateway_lamports.saturating_sub(min_rent);

    // Live Multi-Account Sweep: Sweeps both primary active stake and transient warmup stake
    let stake_rent = Rent::get()?.minimum_balance(std::mem::size_of::<solana_stake_interface::state::StakeStateV2>());
    let mut live_delegated_lamports: u64 = 0;

    for bucket in ctx.accounts.registry.buckets.iter() {
        // 1. Sweep primary active stake
        if bucket.active_lamports > 0 {
            let stake_info = ctx.remaining_accounts.iter().find(|a| a.key == &bucket.stake_account)
                .ok_or(OblioError::InvalidSysvar)?;
            require_keys_eq!(*stake_info.owner, solana_stake_interface::program::ID, OblioError::InvalidStakeProgram);
            let net_stake = stake_info.lamports().saturating_sub(stake_rent);
            live_delegated_lamports = live_delegated_lamports.checked_add(net_stake).ok_or(OblioError::MathOverflow)?;
        }

        // 2. Sweep transient warmup stake to eliminate the warmup multiplier crash
        if bucket.transient_lamports > 0 {
            let transient_info = ctx.remaining_accounts.iter().find(|a| a.key == &bucket.transient_account)
                .ok_or(OblioError::InvalidSysvar)?;
            require_keys_eq!(*transient_info.owner, solana_stake_interface::program::ID, OblioError::InvalidStakeProgram);
            let net_transient = transient_info.lamports().saturating_sub(stake_rent);
            live_delegated_lamports = live_delegated_lamports.checked_add(net_transient).ok_or(OblioError::MathOverflow)?;
        }
    }

    let total_protocol_lamports = live_delegated_lamports
        .checked_add(physical_liquid)
        .ok_or(OblioError::MathOverflow)?;

    let non_share_claims = gateway.subvention_pool_lamports
        .checked_add(gateway.protocol_fee_lamports).ok_or(OblioError::MathOverflow)?
        .checked_add(gateway.pending_ticket_lamports).ok_or(OblioError::MathOverflow)?;

    let total_backing_assets = total_protocol_lamports.saturating_sub(non_share_claims);

    gateway.total_true_sol = total_backing_assets;
    gateway.last_synced_epoch = clock.epoch;

    // Dimensionless 1:1 rebasing rate: (total_true_sol in lamports / total_base_shares in raw units)
    let new_multiplier = if gateway.total_base_shares == 0 {
        1.0_f64
    } else {
        let rate = (gateway.total_true_sol as f64) / (gateway.total_base_shares as f64);
        if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0_f64
        }
    };

    let ix_update = update_multiplier(
        &ctx.accounts.token_2022_program.key(),
        &gateway.conf_sol_mint,
        &ctx.accounts.mint_authority.key(),
        &[&ctx.accounts.mint_authority.key()],
        new_multiplier,
        clock.unix_timestamp,
    )?;

    let authority_seeds: &[&[&[u8]]] = &[&[MINT_AUTHORITY_SEED, &[gateway.mint_authority_bump]]];

    solana_program::program::invoke_signed(
        &ix_update,
        &[
            ctx.accounts.conf_sol_mint.to_account_info(),
            ctx.accounts.mint_authority.to_account_info(),
            ctx.accounts.token_2022_program.to_account_info(),
        ],
        authority_seeds,
    )?;

    emit!(CrankRateUpdatedEvent {
        new_multiplier,
        total_true_sol: gateway.total_true_sol,
        total_base_shares: gateway.total_base_shares,
        epoch: clock.epoch,
    });

    Ok(())
}