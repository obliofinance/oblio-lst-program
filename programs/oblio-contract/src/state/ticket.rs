use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct UnstakeTicket {
    pub orderer: Pubkey,
    pub recipient: Pubkey,
    pub shares_burned: u64,
    pub sol_claimable: u64,
    /// Cumulative queue position (lamports) at which this ticket is fully funded.
    pub queue_end: u128,
    pub creation_epoch: u64,
    pub bump: u8,
}
