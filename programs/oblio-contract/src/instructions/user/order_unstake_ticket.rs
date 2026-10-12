use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Burn, Mint, Token2022, TokenAccount};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::TicketOrderedEvent;
use crate::math::assets_for_shares;
use crate::state::{GatewayState, ProtocolConfig, UnstakeTicket};
use crate::utils::rent_min;

#[derive(Accounts)]
#[instruction(ticket_id: u64)]
pub struct OrderUnstakeTicket<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        constraint = !gateway.breaker_tripped @ OblioError::ProtocolPaused
    )]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        init,
        payer = user,
        space = 8 + UnstakeTicket::INIT_SPACE,
        seeds = [TICKET_SEED, user.key().as_ref(), &ticket_id.to_le_bytes()],
        bump
    )]
    pub ticket: Box<Account<'info, UnstakeTicket>>,

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

    #[account(mut)]
    pub user: Signer<'info>,
    pub token_2022_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

/// `recipient == Pubkey::default()` pays the orderer.
pub fn order_unstake_ticket_handler(
    ctx: Context<OrderUnstakeTicket>,
    ticket_id: u64,
    shares_to_burn: u64,
    recipient: Pubkey,
    min_sol_claimable: u64,
) -> Result<()> {
    let clock = Clock::get()?;
    let params = ctx.accounts.config.params;
    let gateway_key = ctx.accounts.gateway.key();
    let gateway = &mut ctx.accounts.gateway;
    gateway.require_fresh(clock.epoch)?;
    require_gt!(shares_to_burn, 0u64, OblioError::ZeroAmount);

    let supply = ctx.accounts.conf_sol_mint.supply;
    let supply_after = supply.checked_sub(shares_to_burn).ok_or(OblioError::MathOverflow)?;
    require!(supply_after == 0 || supply_after >= params.min_total_shares, OblioError::SupplyBelowMinimum);

    let vested = gateway.vested_assets(clock.unix_timestamp)?;
    let claimable = assets_for_shares(shares_to_burn, vested, supply, gateway.virtual_offset_lamports)?;
    require_gte!(claimable, params.min_unstake_lamports, OblioError::UnstakeBelowMinimum);
    require_gte!(claimable, min_sol_claimable, OblioError::SlippageExceeded);

    // effects: liability leaves share backing and joins the FIFO queue
    gateway.total_assets = gateway.total_assets.checked_sub(claimable).ok_or(OblioError::MathOverflow)?;
    gateway.ticket_queue_ordered = gateway
        .ticket_queue_ordered
        .checked_add(claimable as u128)
        .ok_or(OblioError::MathOverflow)?;

    let final_recipient = if recipient == Pubkey::default() { ctx.accounts.user.key() } else { recipient };
    require_keys_neq!(final_recipient, gateway_key, OblioError::InvalidRecipient);
    let ticket = &mut ctx.accounts.ticket;
    ticket.orderer = ctx.accounts.user.key();
    ticket.recipient = final_recipient;
    ticket.shares_burned = shares_to_burn;
    ticket.sol_claimable = claimable;
    ticket.queue_end = gateway.ticket_queue_ordered;
    ticket.creation_epoch = clock.epoch;
    ticket.bump = ctx.bumps.ticket;

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

    // fund immediately if free liquidity (or excess reserve) exists
    let gw = ctx.accounts.gateway.to_account_info();
    let rent = rent_min(&gw)?;
    ctx.accounts.gateway.settle_liquidity(gw.lamports(), rent, params.reserve_ratio_bps)?;

    emit!(TicketOrderedEvent {
        orderer: ctx.accounts.user.key(),
        recipient: final_recipient,
        ticket_id,
        shares_burned: shares_to_burn,
        sol_claimable: claimable,
        queue_end: ctx.accounts.ticket.queue_end,
        creation_epoch: clock.epoch,
    });
    Ok(())
}
