use anchor_lang::prelude::*;
use crate::state::gateway::GatewayState;
use crate::constants::GATEWAY_SEED;
use crate::errors::OblioError;
use crate::events::ProtocolFeesClaimedEvent;

#[derive(Accounts)]
pub struct ClaimProtocolFees<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        has_one = admin @ OblioError::Unauthorized
    )]
    pub gateway: Account<'info, GatewayState>,

    #[account(mut)]
    pub admin: Signer<'info>,
}

pub fn claim_protocol_fees_handler(ctx: Context<ClaimProtocolFees>, amount: u64) -> Result<()> {
    let gateway = &mut ctx.accounts.gateway;
    require!(amount > 0 && amount <= gateway.protocol_fee_lamports, OblioError::InsufficientFunds);

    let min_rent = Rent::get()?.minimum_balance(gateway.to_account_info().data_len());
    let gateway_balance = gateway.to_account_info().lamports();
    require!(
        gateway_balance.saturating_sub(amount) >= min_rent,
        OblioError::RentExemptionBreached
    );

    gateway.protocol_fee_lamports = gateway.protocol_fee_lamports.saturating_sub(amount);

    **gateway.to_account_info().try_borrow_mut_lamports()? -= amount;
    **ctx.accounts.admin.to_account_info().try_borrow_mut_lamports()? += amount;

    emit!(ProtocolFeesClaimedEvent {
        admin: ctx.accounts.admin.key(),
        lamports: amount,
    });

    Ok(())
}