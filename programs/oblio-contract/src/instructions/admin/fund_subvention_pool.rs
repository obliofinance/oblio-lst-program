use anchor_lang::prelude::*;
use crate::state::gateway::GatewayState;
use crate::constants::GATEWAY_SEED;
use crate::errors::OblioError;
use crate::events::SubventionPoolFundedEvent;

#[derive(Accounts)]
pub struct FundSubventionPool<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump
    )]
    pub gateway: Account<'info, GatewayState>,

    #[account(mut)]
    pub funder: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn fund_subvention_pool_handler(ctx: Context<FundSubventionPool>, lamports: u64) -> Result<()> {
    require!(lamports > 0, OblioError::MathOverflow);

    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.key(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.funder.to_account_info(),
                to: ctx.accounts.gateway.to_account_info(),
            },
        ),
        lamports,
    )?;

    let gateway = &mut ctx.accounts.gateway;
    gateway.subvention_pool_lamports = gateway
        .subvention_pool_lamports
        .checked_add(lamports)
        .ok_or(OblioError::MathOverflow)?;

    emit!(SubventionPoolFundedEvent {
        funder: ctx.accounts.funder.key(),
        lamports,
        new_balance: gateway.subvention_pool_lamports,
    });

    Ok(())
}