use anchor_lang::prelude::*;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::*;
use crate::state::{GatewayState, ProtocolConfig, ProtocolParams};

#[derive(Accounts)]
pub struct ProposeParams<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Account<'info, ProtocolConfig>,
    pub admin: Signer<'info>,
}

pub fn propose_params_handler(ctx: Context<ProposeParams>, params: ProtocolParams) -> Result<()> {
    params.validate()?;
    let c = &mut ctx.accounts.config;
    let eta = Clock::get()?
        .unix_timestamp
        .checked_add(c.params.timelock_secs)
        .ok_or(OblioError::MathOverflow)?;
    c.pending_params = Some(params);
    c.pending_params_eta = eta;
    emit!(ParamsProposedEvent { params, executable_at: eta });
    Ok(())
}

#[derive(Accounts)]
pub struct ExecuteParams<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Account<'info, GatewayState>,
    #[account(seeds = [MINT_SEED], bump = gateway.mint_bump)]
    pub conf_sol_mint: InterfaceAccount<'info, anchor_spl::token_interface::Mint>,
    pub admin: Signer<'info>,
}

pub fn execute_params_handler(ctx: Context<ExecuteParams>) -> Result<()> {
    let c = &mut ctx.accounts.config;
    let params = c.pending_params.ok_or(OblioError::NoPendingParams)?;
    require_gte!(Clock::get()?.unix_timestamp, c.pending_params_eta, OblioError::TimelockNotElapsed);
    require_gte!(params.max_buckets, ctx.accounts.gateway.bucket_count, OblioError::InvalidParams);
    // A floor above the live supply would make every partial exit revert.
    let supply = ctx.accounts.conf_sol_mint.supply;
    require!(supply == 0 || params.min_total_shares <= supply, OblioError::InvalidParams);
    c.params = params;
    c.pending_params = None;
    c.pending_params_eta = 0;
    emit!(ParamsExecutedEvent { params });
    Ok(())
}

#[derive(Accounts)]
pub struct CancelParams<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Account<'info, ProtocolConfig>,
    pub admin: Signer<'info>,
}

pub fn cancel_params_handler(ctx: Context<CancelParams>) -> Result<()> {
    let c = &mut ctx.accounts.config;
    require!(c.pending_params.is_some(), OblioError::NoPendingParams);
    c.pending_params = None;
    c.pending_params_eta = 0;
    emit!(ParamsCancelledEvent { admin: ctx.accounts.admin.key() });
    Ok(())
}

#[derive(Accounts)]
pub struct ProposeAction<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Account<'info, ProtocolConfig>,
    pub admin: Signer<'info>,
}

pub fn propose_action_handler(ctx: Context<ProposeAction>, action: crate::state::TimelockedAction) -> Result<()> {
    let c = &mut ctx.accounts.config;
    let eta = Clock::get()?
        .unix_timestamp
        .checked_add(c.params.timelock_secs)
        .ok_or(OblioError::MathOverflow)?;
    c.pending_action = Some(action);
    c.pending_action_eta = eta;
    emit!(ActionProposedEvent { executable_at: eta });
    Ok(())
}

pub fn cancel_action_handler(ctx: Context<ProposeAction>) -> Result<()> {
    let c = &mut ctx.accounts.config;
    require!(c.pending_action.take().is_some(), OblioError::NoPendingAction);
    c.pending_action_eta = 0;
    emit!(ActionCancelledEvent { admin: ctx.accounts.admin.key() });
    Ok(())
}
