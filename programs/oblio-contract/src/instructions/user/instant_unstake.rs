use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Burn, Mint, Token2022, TokenAccount};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::InstantUnstakeEvent;
use crate::math::{assets_for_shares, instant_exit_fee};
use crate::state::{GatewayState, ProtocolConfig};
use crate::utils::{pay_out, rent_min};

#[derive(Accounts)]
pub struct InstantUnstake<'info> {
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
        token::mint = conf_sol_mint,
        token::authority = user,
        token::token_program = token_2022_program,
    )]
    pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [MINT_SEED],
        bump = gateway.mint_bump,
        mint::token_program = token_2022_program,
    )]
    pub conf_sol_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: any writable account (wallets or program-owned PDAs); rent-exemption is enforced on payout.
    #[account(mut, constraint = recipient.key() != gateway.key() @ OblioError::InvalidRecipient)]
    pub recipient: UncheckedAccount<'info>,

    #[account(mut)]
    pub user: Signer<'info>,
    pub token_2022_program: Program<'info, Token2022>,
}

pub fn instant_unstake_handler(ctx: Context<InstantUnstake>, shares_to_burn: u64, min_net_lamports: u64) -> Result<()> {
    let clock = Clock::get()?;
    let params = ctx.accounts.config.params;
    let gateway = &mut ctx.accounts.gateway;
    gateway.require_fresh(clock.epoch)?;
    require_gt!(shares_to_burn, 0u64, OblioError::ZeroAmount);

    let supply = ctx.accounts.conf_sol_mint.supply;
    let supply_after = supply.checked_sub(shares_to_burn).ok_or(OblioError::MathOverflow)?;
    require!(supply_after == 0 || supply_after >= params.min_total_shares, OblioError::SupplyBelowMinimum);

    let vested = gateway.vested_assets(clock.unix_timestamp)?;
    let gross = assets_for_shares(shares_to_burn, vested, supply, gateway.virtual_offset_lamports)?;
    require_gte!(gross, params.min_unstake_lamports, OblioError::UnstakeBelowMinimum);

    // Fee integrates the curve over the reserve the trade consumes (post-trade utilisation).
    let target = gateway.target_reserve(params.reserve_ratio_bps)?;
    let fee = instant_exit_fee(gross, gateway.reserve_pool_lamports, target, &params)?;
    let net = gross - fee;
    require_gte!(net, min_net_lamports, OblioError::SlippageExceeded);

    let protocol_part = (fee as u128 * params.protocol_fee_share_bps as u128 / BPS_DENOMINATOR as u128) as u64;
    let leaves_reserve = net.checked_add(protocol_part).ok_or(OblioError::MathOverflow)?;
    require_gte!(gateway.reserve_pool_lamports, leaves_reserve, OblioError::ReservePoolDepleted);

    // effects: the retained part of the fee stays in the pool and accrues to remaining holders
    gateway.reserve_pool_lamports -= leaves_reserve;
    gateway.total_assets = gateway.total_assets.checked_sub(leaves_reserve).ok_or(OblioError::MathOverflow)?;
    gateway.protocol_fee_lamports =
        gateway.protocol_fee_lamports.checked_add(protocol_part).ok_or(OblioError::MathOverflow)?;

    // interactions
    token_interface::burn(
        CpiContext::new(
            ctx.accounts.token_2022_program.key(),
            Burn {
                mint: ctx.accounts.conf_sol_mint.to_account_info(),
                from: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        shares_to_burn,
    )?;
    let gw = gateway.to_account_info();
    pay_out(&gw, &ctx.accounts.recipient.to_account_info(), net)?;
    gateway.assert_solvent(gw.lamports(), rent_min(&gw)?)?;

    emit!(InstantUnstakeEvent {
        user: ctx.accounts.user.key(),
        recipient: ctx.accounts.recipient.key(),
        shares_burned: shares_to_burn,
        gross_lamports: gross,
        fee_lamports: fee,
        protocol_fee_lamports: protocol_part,
        net_lamports: net,
    });
    Ok(())
}
