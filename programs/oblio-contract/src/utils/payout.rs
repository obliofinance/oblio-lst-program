use anchor_lang::prelude::*;
use crate::errors::OblioError;

/// Minimum balance for an account of `info`'s current data length.
pub fn rent_min(info: &AccountInfo) -> Result<u64> {
    Ok(Rent::get()?.minimum_balance(info.data_len()))
}

/// Native lamport transfer out of a program-owned account with checked arithmetic
/// (Anchor `Lamports` trait). Recipient may be any writable account, including program-owned PDAs.
pub fn pay_out<'info>(from: &AccountInfo<'info>, to: &AccountInfo<'info>, amount: u64) -> Result<()> {
    let min = rent_min(to)?;
    let after = to.lamports().checked_add(amount).ok_or(OblioError::MathOverflow)?;
    require_gte!(after, min, OblioError::RecipientBelowRentExemption);
    from.sub_lamports(amount)?;
    to.add_lamports(amount)?;
    Ok(())
}
