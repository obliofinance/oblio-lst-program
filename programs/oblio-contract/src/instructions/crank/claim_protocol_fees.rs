use anchor_lang::prelude::*;
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::ProtocolFeesClaimedEvent;
use crate::state::{GatewayState, ProtocolConfig};
use crate::utils::{pay_out, rent_min};

/// Permissionless: fees can only ever flow to the configured recipient.
#[derive(Accounts)]
pub struct ClaimProtocolFees<'info> {
    #[account(mut, seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Box<Account<'info, GatewayState>>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,
    /// CHECK: pinned to `config.fee_recipient`.
    #[account(mut, address = config.fee_recipient)]
    pub fee_recipient: UncheckedAccount<'info>,
}

pub fn claim_protocol_fees_handler(ctx: Context<ClaimProtocolFees>) -> Result<()> {
    let g = &mut ctx.accounts.gateway;
    let amount = g.protocol_fee_lamports;
    require_gt!(amount, 0u64, OblioError::NothingToClaim);
    g.protocol_fee_lamports = 0;

    let gw = g.to_account_info();
    pay_out(&gw, &ctx.accounts.fee_recipient.to_account_info(), amount)?;
    g.assert_solvent(gw.lamports(), rent_min(&gw)?)?;
    emit!(ProtocolFeesClaimedEvent { recipient: ctx.accounts.fee_recipient.key(), lamports: amount });
    Ok(())
}
