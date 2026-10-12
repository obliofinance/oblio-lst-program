use anchor_lang::{
    prelude::*,
    solana_program::program::invoke,
    system_program::{self, CreateAccount},
};
use anchor_spl::{
    token_2022::spl_token_2022::{
        self,
        extension::{
            confidential_transfer::instruction::initialize_mint as init_conf_mint,
            scaled_ui_amount::instruction::initialize as init_scaled_ui_amount, ExtensionType,
        },
    },
    token_interface::{
        self, InitializeMint2, MetadataPointerInitialize, Token2022, TokenMetadataInitialize,
    },
};
use crate::constants::*;
use crate::events::ProtocolInitializedEvent;
use crate::state::{GatewayState, ProtocolConfig, ProtocolParams};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitializeArgs {
    pub admin: Pubkey,
    pub guardian: Pubkey,
    pub fee_recipient: Pubkey,
    pub params: ProtocolParams,
    /// Immutable virtual offset (lamports) for the share price.
    pub virtual_offset_lamports: u64,
    pub auto_approve_confidential: bool,
    pub name: String,
    pub symbol: String,
    pub uri: String,
}

#[derive(Accounts)]
#[instruction(args: InitializeArgs)]
pub struct InitializeProtocol<'info> {
    #[account(init, payer = authority, space = 8 + ProtocolConfig::INIT_SPACE, seeds = [CONFIG_SEED], bump)]
    pub config: Box<Account<'info, ProtocolConfig>>,

    #[account(init, payer = authority, space = 8 + GatewayState::INIT_SPACE, seeds = [GATEWAY_SEED], bump)]
    pub gateway: Box<Account<'info, GatewayState>>,

    /// CHECK: canonical PDA; created and initialised as a Token-2022 mint in the handler.
    #[account(mut, seeds = [MINT_SEED], bump)]
    pub conf_sol_mint: UncheckedAccount<'info>,

    /// CHECK: PDA that owns mint / scaled-UI / confidential-transfer authorities.
    #[account(seeds = [MINT_AUTHORITY_SEED], bump)]
    pub mint_authority: UncheckedAccount<'info>,

    /// Only the program's upgrade authority may initialise (front-running protection).
    #[account(mut, constraint = program_data.upgrade_authority_address == Some(authority.key()) @ crate::errors::OblioError::Unauthorized)]
    pub authority: Signer<'info>,

    #[account(constraint = program.programdata_address()? == Some(program_data.key()) @ crate::errors::OblioError::Unauthorized)]
    pub program: Program<'info, crate::program::OblioContract>,
    pub program_data: Account<'info, ProgramData>,

    pub system_program: Program<'info, System>,
    pub token_2022_program: Program<'info, Token2022>,
}

pub fn initialize_protocol_handler(ctx: Context<InitializeProtocol>, args: InitializeArgs) -> Result<()> {
    args.params.validate()?;
    require_gte!(args.virtual_offset_lamports, MIN_VIRTUAL_OFFSET_LAMPORTS, crate::errors::OblioError::InvalidParams);
    for key in [args.admin, args.guardian, args.fee_recipient] {
        require_keys_neq!(key, Pubkey::default(), crate::errors::OblioError::InvalidParams);
    }

    let c = &mut ctx.accounts.config;
    c.admin = args.admin;
    c.pending_admin = None;
    c.guardian = args.guardian;
    c.fee_recipient = args.fee_recipient;
    c.params = args.params;
    c.pending_params = None;
    c.pending_params_eta = 0;
    c.pending_action = None;
    c.pending_action_eta = 0;
    c.version = 1;
    c.bump = ctx.bumps.config;

    let g = &mut ctx.accounts.gateway;
    g.bump = ctx.bumps.gateway;
    g.mint_bump = ctx.bumps.conf_sol_mint;
    g.mint_authority_bump = ctx.bumps.mint_authority;
    g.virtual_offset_lamports = args.virtual_offset_lamports;
    g.last_synced_epoch = Clock::get()?.epoch;
    g.last_rate_wad = WAD as u64;
    g.version = 1;

    // ---- create the mint (PDA) sized for every extension ----
    let ext = [
        ExtensionType::ScaledUiAmount,
        ExtensionType::ConfidentialTransferMint,
        ExtensionType::MetadataPointer,
    ];
    let base_space = ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&ext)?;
    // TLV header + update_authority + mint + 3 string prefixes + strings + additional_metadata prefix (+slack)
    let metadata_space = 4 + 32 + 32 + 12 + args.name.len() + args.symbol.len() + args.uri.len() + 4 + 32;
    let lamports = Rent::get()?.minimum_balance(base_space + metadata_space);

    let mint_seeds: &[&[&[u8]]] = &[&[MINT_SEED, &[ctx.bumps.conf_sol_mint]]];
    system_program::create_account(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.key(),
            CreateAccount {
                from: ctx.accounts.authority.to_account_info(),
                to: ctx.accounts.conf_sol_mint.to_account_info(),
            },
            mint_seeds,
        ),
        lamports,
        base_space as u64,
        &ctx.accounts.token_2022_program.key(),
    )?;

    let mint_info = ctx.accounts.conf_sol_mint.to_account_info();
    let tp = ctx.accounts.token_2022_program.to_account_info();
    let ma = ctx.accounts.mint_authority.key();

    // MetadataPointer: immutable, points at the mint itself.
    token_interface::metadata_pointer_initialize(
        CpiContext::new(
            tp.key(),
            MetadataPointerInitialize { token_program_id: tp.clone(), mint: mint_info.clone() },
        ),
        None,
        Some(mint_info.key()),
    )?;

    // ScaledUiAmount: rate starts at exactly 1.0.
    let ix = init_scaled_ui_amount(&tp.key(), &mint_info.key(), Some(ma), 1.0_f64)?;
    invoke(&ix, &[mint_info.clone(), tp.clone()])?;

    // ConfidentialTransfer: PDA authority; auditor can be set later via `update_confidential_mint`.
    let ix = init_conf_mint(&tp.key(), &mint_info.key(), Some(ma), args.auto_approve_confidential, None)?;
    invoke(&ix, &[mint_info.clone(), tp.clone()])?;

    token_interface::initialize_mint2(
        CpiContext::new(tp.key(), InitializeMint2 { mint: mint_info.clone() }),
        DECIMALS,
        &ma,
        None,
    )?;

    let authority_seeds: &[&[&[u8]]] = &[&[MINT_AUTHORITY_SEED, &[ctx.bumps.mint_authority]]];
    token_interface::token_metadata_initialize(
        CpiContext::new_with_signer(
            tp.key(),
            TokenMetadataInitialize {
                program_id: tp.clone(),
                mint: mint_info.clone(),
                metadata: mint_info.clone(),
                mint_authority: ctx.accounts.mint_authority.to_account_info(),
                update_authority: ctx.accounts.mint_authority.to_account_info(),
            },
            authority_seeds,
        ),
        args.name,
        args.symbol,
        args.uri,
    )?;

    emit!(ProtocolInitializedEvent {
        admin: args.admin,
        mint: mint_info.key(),
        params: args.params,
    });
    Ok(())
}
