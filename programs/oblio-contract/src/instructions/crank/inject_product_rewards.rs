use anchor_lang::prelude::*;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::RewardsInjectedEvent;
use crate::state::{GatewayState, ProtocolConfig};
use crate::utils::rent_min;

#[derive(Accounts)]
pub struct InjectProductRewards<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        constraint = !gateway.paused @ OblioError::ProtocolPaused
    )]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,
    #[account(seeds = [MINT_SEED], bump = gateway.mint_bump)]
    pub conf_sol_mint: Box<InterfaceAccount<'info, anchor_spl::token_interface::Mint>>,
    #[account(mut)]
    pub reward_funder: Signer<'info>,
    pub system_program: Program<'info, System>,
}

/// External rewards (MEV, partner incentives). They enter `total_assets` but VEST linearly,
/// so deposit -> inject -> withdraw sniping is impossible.
pub fn inject_product_rewards_handler(ctx: Context<InjectProductRewards>, lamports: u64) -> Result<()> {
    let params = ctx.accounts.config.params;
    require_gte!(lamports, params.min_reward_injection_lamports, OblioError::ZeroAmount);
    require_gt!(ctx.accounts.conf_sol_mint.supply, 0u64, OblioError::NoSupply);

    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.key(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.reward_funder.to_account_info(),
                to: ctx.accounts.gateway.to_account_info(),
            },
        ),
        lamports,
    )?;

    let now = Clock::get()?.unix_timestamp;
    let g = &mut ctx.accounts.gateway;
    g.total_assets = g.total_assets.checked_add(lamports).ok_or(OblioError::MathOverflow)?;
    g.lock_profit(lamports, now, params.profit_vesting_secs)?;
    let rent = rent_min(&ctx.accounts.gateway.to_account_info())?;
    ctx.accounts.gateway.assert_solvent(ctx.accounts.gateway.get_lamports(), rent)?;
    emit!(RewardsInjectedEvent { caller: ctx.accounts.reward_funder.key(), lamports });
    Ok(())
}
