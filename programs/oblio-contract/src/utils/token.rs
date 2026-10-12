use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::{
    token_2022::spl_token_2022::extension::confidential_transfer::instruction::deposit as confidential_deposit,
    token_interface::{self, Mint, MintTo, Token2022, TokenAccount},
};
use crate::constants::*;
use crate::state::MintMode;

/// Mints `shares` to `to` (PDA mint authority) and optionally moves them into the confidential balance.
#[allow(clippy::too_many_arguments)]
pub fn mint_shares<'info>(
    token_program: &Program<'info, Token2022>,
    mint: &InterfaceAccount<'info, Mint>,
    to: &InterfaceAccount<'info, TokenAccount>,
    mint_authority: &UncheckedAccount<'info>,
    mint_authority_bump: u8,
    owner: &Signer<'info>,
    shares: u64,
    mode: MintMode,
) -> Result<()> {
    let seeds: &[&[&[u8]]] = &[&[MINT_AUTHORITY_SEED, &[mint_authority_bump]]];
    token_interface::mint_to(
        CpiContext::new_with_signer(
            token_program.key(),
            MintTo {
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority: mint_authority.to_account_info(),
            },
            seeds,
        ),
        shares,
    )?;

    if mode == MintMode::Confidential {
        let ix = confidential_deposit(
            &token_program.key(),
            &to.key(),
            &mint.key(),
            shares,
            DECIMALS,
            &owner.key(),
            &[],
        )?;
        invoke(
            &ix,
            &[
                to.to_account_info(),
                mint.to_account_info(),
                owner.to_account_info(),
                token_program.to_account_info(),
            ],
        )?;
    }
    Ok(())
}
