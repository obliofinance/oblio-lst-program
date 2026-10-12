use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::{
    token_2022::spl_token_2022::extension::confidential_transfer::instruction::update_mint,
    token_interface::{Mint, Token2022},
};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::ConfidentialMintUpdatedEvent;
use crate::state::{GatewayState, ProtocolConfig, TimelockedAction};

#[derive(Accounts)]
pub struct UpdateConfidentialMint<'info> {
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ OblioError::Unauthorized)]
    pub config: Account<'info, ProtocolConfig>,
    #[account(seeds = [GATEWAY_SEED], bump = gateway.bump)]
    pub gateway: Account<'info, GatewayState>,
    #[account(mut, seeds = [MINT_SEED], bump = gateway.mint_bump, mint::token_program = token_2022_program)]
    pub conf_sol_mint: InterfaceAccount<'info, Mint>,
    /// CHECK: PDA confidential-transfer authority.
    #[account(seeds = [MINT_AUTHORITY_SEED], bump = gateway.mint_authority_bump)]
    pub mint_authority: UncheckedAccount<'info>,
    pub admin: Signer<'info>,
    pub token_2022_program: Program<'info, Token2022>,
}

/// Lets institutions configure a compliance auditor and the auto-approve policy.
pub fn update_confidential_mint_handler(
    ctx: Context<UpdateConfidentialMint>,
    auto_approve: bool,
    auditor_elgamal_pubkey: Option<[u8; 32]>,
) -> Result<()> {
    ctx.accounts.config.consume_action(
        Clock::get()?.unix_timestamp,
        &TimelockedAction::UpdateConfidentialMint { auto_approve, auditor_elgamal_pubkey },
    )?;
    let auditor = auditor_elgamal_pubkey.map(|b| bytemuck::pod_read_unaligned(&b));
    let ix = update_mint(
        &ctx.accounts.token_2022_program.key(),
        &ctx.accounts.conf_sol_mint.key(),
        &ctx.accounts.mint_authority.key(),
        &[],
        auto_approve,
        auditor,
    )?;
    let seeds: &[&[&[u8]]] = &[&[MINT_AUTHORITY_SEED, &[ctx.accounts.gateway.mint_authority_bump]]];
    invoke_signed(
        &ix,
        &[
            ctx.accounts.conf_sol_mint.to_account_info(),
            ctx.accounts.mint_authority.to_account_info(),
            ctx.accounts.token_2022_program.to_account_info(),
        ],
        seeds,
    )?;
    emit!(ConfidentialMintUpdatedEvent { auto_approve, has_auditor: auditor_elgamal_pubkey.is_some() });
    Ok(())
}
