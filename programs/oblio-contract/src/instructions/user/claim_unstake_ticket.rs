use anchor_lang::prelude::*;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::TicketClaimedEvent;
use crate::state::{GatewayState, ProtocolConfig, UnstakeTicket};
use crate::utils::{pay_out, rent_min};

/// Permissionless (relayer friendly). Works while paused so exits are never trapped.
#[derive(Accounts)]
#[instruction(ticket_id: u64)]
pub struct ClaimUnstakeTicket<'info> {
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        mut,
        close = recipient,
        seeds = [TICKET_SEED, ticket.orderer.as_ref(), &ticket_id.to_le_bytes()],
        bump = ticket.bump,
        has_one = recipient @ OblioError::Unauthorized,
    )]
    pub ticket: Box<Account<'info, UnstakeTicket>>,

    /// CHECK: pinned to `ticket.recipient` via `has_one`; may be a wallet or a program-owned PDA.
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
}

pub fn claim_unstake_ticket_handler(ctx: Context<ClaimUnstakeTicket>, _ticket_id: u64) -> Result<()> {
    let clock = Clock::get()?;
    let params = ctx.accounts.config.params;
    let ticket = &ctx.accounts.ticket;
    let unlock = ticket
        .creation_epoch
        .checked_add(params.ticket_cooldown_epochs)
        .ok_or(OblioError::MathOverflow)?;
    require_gte!(clock.epoch, unlock, OblioError::TicketStillLocked);

    let gateway = &mut ctx.accounts.gateway;
    require_gte!(gateway.ticket_queue_funded, ticket.queue_end, OblioError::TicketNotFunded);

    let payout = ticket.sol_claimable;
    gateway.ticket_queue_claimed = gateway
        .ticket_queue_claimed
        .checked_add(payout as u128)
        .ok_or(OblioError::MathOverflow)?;

    let gw = gateway.to_account_info();
    pay_out(&gw, &ctx.accounts.recipient.to_account_info(), payout)?;
    gateway.assert_solvent(gw.lamports(), rent_min(&gw)?)?;

    emit!(TicketClaimedEvent {
        recipient: ctx.accounts.recipient.key(),
        ticket: ctx.accounts.ticket.key(),
        lamports_paid: payout,
    });
    Ok(())
}
