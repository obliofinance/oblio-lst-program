use anchor_lang::prelude::*;
use crate::constants::*;
use crate::events::LiquiditySettledEvent;
use crate::state::{GatewayState, ProtocolConfig};
use crate::utils::rent_min;

#[derive(Accounts)]
pub struct SettleLiquidity<'info> {
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Account<'info, GatewayState>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, ProtocolConfig>,
}

/// Permissionless: funds the ticket queue (FIFO) and refills the reserve from free liquidity.
pub fn settle_liquidity_handler(ctx: Context<SettleLiquidity>) -> Result<()> {
    let physical = ctx.accounts.gateway.get_lamports();
    let rent = rent_min(&ctx.accounts.gateway.to_account_info())?;
    let ratio = ctx.accounts.config.params.reserve_ratio_bps;
    let g = &mut ctx.accounts.gateway;
    let free = g.settle_liquidity(physical, rent, ratio)?;
    emit!(LiquiditySettledEvent { tickets_funded: g.ticket_queue_funded, reserve: g.reserve_pool_lamports, free });
    Ok(())
}
