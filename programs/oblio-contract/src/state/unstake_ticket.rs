use anchor_lang::prelude::*;

#[account]
pub struct UnstakeTicket {
    pub orderer: Pubkey,
    pub recipient: Pubkey,
    pub shares_burned: u64,
    pub sol_claimable: u64,
    pub creation_epoch: u64,
    pub is_claimed: bool,
    pub bump: u8,
}

impl UnstakeTicket {
    pub const LEN: usize = 8  // Discriminator
        + 32 // orderer
        + 32 // recipient
        + 8  // shares_burned
        + 8  // sol_claimable
        + 8  // creation_epoch
        + 1  // is_claimed
        + 1; // bump
}