use anchor_lang::prelude::*;

/// Whether freshly minted shares are moved into the confidential (pending) balance atomically.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MintMode {
    Public,
    Confidential,
}
