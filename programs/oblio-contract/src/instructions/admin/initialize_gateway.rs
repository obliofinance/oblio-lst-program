use anchor_lang::{prelude::*, solana_program};
use anchor_spl::token_2022::spl_token_2022::{
    extension::{
        scaled_ui_amount::instruction::initialize as init_scaled_ui_amount,
        confidential_transfer::instruction::initialize_mint as init_conf_mint,
    },
    instruction::initialize_mint2,
};
use crate::state::gateway::GatewayState;
use crate::constants::*;
use crate::events::GatewayInitializedEvent;

#[derive(Accounts)]
pub struct InitializeGateway<'info> {
    #[account(
        init,
        payer = admin,
        space = GatewayState::LEN,
        seeds = [GATEWAY_SEED],
        bump
    )]
    pub gateway: Account<'info, GatewayState>,

    /// CHECK: Uninitialized Token-2022 mint account
    #[account(mut)]
    pub conf_sol_mint: Signer<'info>,

    /// CHECK: PDA authorized to sign for minting
    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump
    )]
    pub mint_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
    /// CHECK: Token-2022 Program ID
    #[account(address = anchor_spl::token_2022::ID)]
    pub token_2022_program: UncheckedAccount<'info>,
}

pub fn initialize_gateway_handler(
    ctx: Context<InitializeGateway>,
    initial_subvention_lamports: u64,
) -> Result<()> {
    let gateway = &mut ctx.accounts.gateway;
    gateway.admin = ctx.accounts.admin.key();
    gateway.pending_admin = Pubkey::default();
    gateway.is_paused = false;
    gateway.conf_sol_mint = ctx.accounts.conf_sol_mint.key();
    gateway.mint_authority_bump = ctx.bumps.mint_authority;
    gateway.total_true_sol = 0;
    gateway.total_base_shares = 0;
    gateway.subvention_pool_lamports = initial_subvention_lamports;
    gateway.reserve_pool_lamports = 0;
    gateway.target_reserve_lamports = 0;
    gateway.protocol_fee_lamports = 0;
    gateway.pending_ticket_lamports = 0;
    gateway.last_synced_epoch = Clock::get()?.epoch;
    gateway.bump = ctx.bumps.gateway;

    if initial_subvention_lamports > 0 {
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.admin.to_account_info(),
                    to: ctx.accounts.gateway.to_account_info(),
                },
            ),
            initial_subvention_lamports,
        )?;
    }

    // 1. Initialize ScaledUiAmount extension (1.0 initial peg)
    let ix_scaled_ui = init_scaled_ui_amount(
        &ctx.accounts.token_2022_program.key(),
        &ctx.accounts.conf_sol_mint.key(),
        Some(ctx.accounts.mint_authority.key()),
        1.0_f64,
    )?;
    solana_program::program::invoke(&ix_scaled_ui, &[
        ctx.accounts.conf_sol_mint.to_account_info(),
        ctx.accounts.token_2022_program.to_account_info(),
    ])?;

    // 2. Initialize ConfidentialTransfer extension with auto_approve = true
    let ix_conf_mint = init_conf_mint(
        &ctx.accounts.token_2022_program.key(),
        &ctx.accounts.conf_sol_mint.key(),
        Some(ctx.accounts.mint_authority.key()),
        true,
        None,
    )?;
    solana_program::program::invoke(&ix_conf_mint, &[
        ctx.accounts.conf_sol_mint.to_account_info(),
        ctx.accounts.token_2022_program.to_account_info(),
    ])?;

    // 3. Initialize Mint with 9 decimals matching native SOL
    let ix_mint = initialize_mint2(
        &ctx.accounts.token_2022_program.key(),
        &ctx.accounts.conf_sol_mint.key(),
        &ctx.accounts.mint_authority.key(),
        None,
        DECIMALS,
    )?;
    solana_program::program::invoke(&ix_mint, &[
        ctx.accounts.conf_sol_mint.to_account_info(),
        ctx.accounts.token_2022_program.to_account_info(),
    ])?;

    emit!(GatewayInitializedEvent {
        admin: ctx.accounts.admin.key(),
        conf_sol_mint: ctx.accounts.conf_sol_mint.key(),
        subvention_pool: initial_subvention_lamports,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}