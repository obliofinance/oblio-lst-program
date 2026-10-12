use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::DepositDirectEvent;
use crate::math::shares_for_assets;
use crate::state::{GatewayState, MintMode, ProtocolConfig};
use crate::utils::{mint_shares, rent_min};

#[derive(Accounts)]
pub struct DepositDirect<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        constraint = !gateway.paused @ OblioError::ProtocolPaused
    )]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,

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

    pub system_program: Program<'info, System>,
    pub token_2022_program: Program<'info, Token2022>,
}

pub fn deposit_direct_handler(
    ctx: Context<DepositDirect>,
    lamports: u64,
    mode: MintMode,
    min_shares_out: u64,
) -> Result<()> {
    let clock = Clock::get()?;
    let params = ctx.accounts.config.params;
    let gateway = &mut ctx.accounts.gateway;
    gateway.require_fresh(clock.epoch)?;
    require_gte!(lamports, params.min_deposit_lamports, OblioError::DepositBelowMinimum);

    let new_total = gateway.total_assets.checked_add(lamports).ok_or(OblioError::MathOverflow)?;
    require_gte!(params.tvl_cap_lamports, new_total, OblioError::TvlCapExceeded);

    // Price from mint supply (source of truth) and *vested* assets.
    let supply = ctx.accounts.conf_sol_mint.supply;
    let vested = gateway.vested_assets(clock.unix_timestamp)?;
    let shares = shares_for_assets(lamports, vested, supply, gateway.virtual_offset_lamports)?;
    require_gt!(shares, 0u64, OblioError::ZeroAmount);
    require_gte!(shares, min_shares_out, OblioError::SlippageExceeded);

    // effects
    gateway.total_assets = new_total;

    // interactions
    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.key(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.depositor.to_account_info(),
                to: ctx.accounts.gateway.to_account_info(),
            },
        ),
        lamports,
    )?;
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

    // fresh liquidity first serves the ticket queue, then the reserve, then waits for delegation
    let gw = ctx.accounts.gateway.to_account_info();
    let rent = rent_min(&gw)?;
    ctx.accounts.gateway.settle_liquidity(gw.lamports(), rent, params.reserve_ratio_bps)?;

    emit!(DepositDirectEvent { depositor: ctx.accounts.depositor.key(), lamports, shares_minted: shares, mode });
    Ok(())
}
