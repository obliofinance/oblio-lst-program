use anchor_lang::prelude::*;
use crate::state::gateway::GatewayState;
use crate::constants::GATEWAY_SEED;
use crate::errors::OblioError;
use crate::events::{EmergencyPauseEvent, AdminTransferProposedEvent, AdminTransferAcceptedEvent};

#[derive(Accounts)]
pub struct EmergencyPause<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        has_one = admin @ OblioError::Unauthorized
    )]
    pub gateway: Account<'info, GatewayState>,
    pub admin: Signer<'info>,
}

pub fn emergency_pause_handler(ctx: Context<EmergencyPause>, is_paused: bool) -> Result<()> {
    let gateway = &mut ctx.accounts.gateway;
    gateway.is_paused = is_paused;

    emit!(EmergencyPauseEvent {
        is_paused,
        admin: ctx.accounts.admin.key(),
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct TransferAdmin<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        has_one = admin @ OblioError::Unauthorized
    )]
    pub gateway: Account<'info, GatewayState>,
    pub admin: Signer<'info>,
}

pub fn transfer_admin_handler(ctx: Context<TransferAdmin>, new_admin: Pubkey) -> Result<()> {
    require!(new_admin != Pubkey::default(), OblioError::Unauthorized);
    let gateway = &mut ctx.accounts.gateway;
    gateway.pending_admin = new_admin;

    emit!(AdminTransferProposedEvent {
        current_admin: gateway.admin,
        pending_admin: new_admin,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump
    )]
    pub gateway: Account<'info, GatewayState>,
    pub new_admin: Signer<'info>,
}

pub fn accept_admin_handler(ctx: Context<AcceptAdmin>) -> Result<()> {
    let gateway = &mut ctx.accounts.gateway;
    require!(gateway.pending_admin != Pubkey::default(), OblioError::NoPendingAdmin);
    require!(gateway.pending_admin == ctx.accounts.new_admin.key(), OblioError::Unauthorized);

    let old_admin = gateway.admin;
    gateway.admin = gateway.pending_admin;
    gateway.pending_admin = Pubkey::default();

    emit!(AdminTransferAcceptedEvent {
        old_admin,
        new_admin: gateway.admin,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}