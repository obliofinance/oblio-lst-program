use anchor_lang::prelude::*;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::*;
use crate::state::{ProtocolConfig, TimelockedAction};

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Account<'info, ProtocolConfig>,
    pub admin: Signer<'info>,
}

pub fn transfer_admin_handler(ctx: Context<AdminOnly>, new_admin: Pubkey) -> Result<()> {
    require_keys_neq!(new_admin, Pubkey::default(), OblioError::Unauthorized);
    let c = &mut ctx.accounts.config;
    c.pending_admin = Some(new_admin);
    emit!(AdminTransferProposedEvent { current_admin: c.admin, pending_admin: new_admin });
    Ok(())
}

pub fn cancel_admin_transfer_handler(ctx: Context<AdminOnly>) -> Result<()> {
    let c = &mut ctx.accounts.config;
    require!(c.pending_admin.take().is_some(), OblioError::NoPendingAdmin);
    emit!(AdminTransferCancelledEvent { admin: c.admin });
    Ok(())
}

pub fn set_guardian_handler(ctx: Context<AdminOnly>, guardian: Pubkey) -> Result<()> {
    require_keys_neq!(guardian, Pubkey::default(), OblioError::InvalidParams);
    let c = &mut ctx.accounts.config;
    c.consume_action(Clock::get()?.unix_timestamp, &TimelockedAction::SetGuardian { guardian })?;
    emit!(RoleUpdatedEvent { role: 0, old: c.guardian, new: guardian });
    c.guardian = guardian;
    Ok(())
}

pub fn set_fee_recipient_handler(ctx: Context<AdminOnly>, recipient: Pubkey) -> Result<()> {
    require_keys_neq!(recipient, Pubkey::default(), OblioError::InvalidParams);
    let c = &mut ctx.accounts.config;
    c.consume_action(Clock::get()?.unix_timestamp, &TimelockedAction::SetFeeRecipient { recipient })?;
    emit!(RoleUpdatedEvent { role: 1, old: c.fee_recipient, new: recipient });
    c.fee_recipient = recipient;
    Ok(())
}

#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = config.pending_admin == Some(new_admin.key()) @ OblioError::Unauthorized
    )]
    pub config: Account<'info, ProtocolConfig>,
    pub new_admin: Signer<'info>,
}

pub fn accept_admin_handler(ctx: Context<AcceptAdmin>) -> Result<()> {
    let c = &mut ctx.accounts.config;
    let old_admin = c.admin;
    c.admin = ctx.accounts.new_admin.key();
    c.pending_admin = None;
    emit!(AdminTransferAcceptedEvent { old_admin, new_admin: c.admin });
    Ok(())
}
