use anchor_lang::prelude::*;
use crate::errors::OblioError;

#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BucketStatus {
    /// Receives delegations and stake deposits.
    Active,
    /// Weight is zero; stake is being unwound by `decrease_stake`.
    Draining,
}

/// One validator bucket. Counters are refreshed from real lamports on every `sync_rate`.
#[account]
#[derive(InitSpace)]
pub struct Bucket {
    /// Unique, contiguous, assigned once (0..gateway.bucket_count).
    pub index: u16,
    pub vote_account: Pubkey,
    pub stake_account: Pubkey,
    /// Current top-up stake account (default when the slot is free).
    pub transient_account: Pubkey,
    /// Current deactivating stake account (default when the slot is free).
    pub unstake_account: Pubkey,
    pub status: BucketStatus,
    /// Relative weight; allocation share = weight / gateway.total_weight.
    pub weight_bps: u16,
    pub is_delegated: bool,
    /// True when the whole primary account (not a split) is deactivating.
    pub unstake_in_place: bool,
    pub active_lamports: u64,
    pub transient_lamports: u64,
    pub unstake_lamports: u64,
    pub transient_seed: u64,
    pub unstake_seed: u64,
    pub stake_rent_reserve: u64,
    pub bump: u8,
    pub stake_bump: u8,
}

impl Bucket {
    pub fn staked_lamports(&self) -> Result<u64> {
        self.active_lamports
            .checked_add(self.transient_lamports)
            .ok_or_else(|| error!(OblioError::MathOverflow))
    }
}
