use crate::errors::OblioError;
use anchor_lang::prelude::*;

pub struct SubventionEngine;

impl SubventionEngine {
    /// Applies rounding subsidies with pure integer arithmetic and graceful fallback.
    pub fn calculate_payout_with_subsidy(
        floor_lamports: u64,
        remainder: u128,
        subvention_balance: &mut u64,
    ) -> Result<u64> {
        if remainder > 0 && *subvention_balance >= 1 {
            *subvention_balance = subvention_balance.saturating_sub(1);
            let payout = floor_lamports
                .checked_add(1)
                .ok_or(OblioError::MathOverflow)?;
            return Ok(payout);
        }
        Ok(floor_lamports)
    }
}