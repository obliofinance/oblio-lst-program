use anchor_lang::prelude::*;

/// Mint decimals. Fixed by design: the confSOL raw unit equals one lamport.
#[constant]
pub const DECIMALS: u8 = 9;

/// Mathematical constants (not tunable parameters).
pub const BPS_DENOMINATOR: u64 = 10_000;
pub const WAD: u128 = 1_000_000_000;

/// Accounts consumed per bucket by `sync_rate`: [bucket, primary stake, transient stake, unstake stake].
pub const BUCKET_ACCOUNTS_PER_SYNC: usize = 4;

/// Hard upper bounds that protect governance from foot-guns (tunables live in `ProtocolParams`).
pub const MAX_FEE_CURVE_EXPONENT: u8 = 4;
pub const MAX_TIMELOCK_SECS: i64 = 30 * 24 * 60 * 60;
/// Governance can never remove the delay entirely.
pub const MIN_TIMELOCK_SECS: i64 = 60 * 60;
/// The virtual offset is immutable, so it is floored at init (inflation-attack defence).
pub const MIN_VIRTUAL_OFFSET_LAMPORTS: u64 = 1_000_000;
pub const MAX_VESTING_SECS: i64 = 30 * 24 * 60 * 60;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";
#[constant]
pub const GATEWAY_SEED: &[u8] = b"gateway";
#[constant]
pub const MINT_SEED: &[u8] = b"conf_sol_mint";
#[constant]
pub const MINT_AUTHORITY_SEED: &[u8] = b"mint_authority";
#[constant]
pub const BUCKET_SEED: &[u8] = b"bucket";
#[constant]
pub const STAKE_SEED: &[u8] = b"stake";
#[constant]
pub const TRANSIENT_SEED: &[u8] = b"transient";
#[constant]
pub const UNSTAKE_SEED: &[u8] = b"unstake";
#[constant]
pub const TICKET_SEED: &[u8] = b"ticket";

pub const STAKE_PROGRAM_ID: Pubkey = solana_stake_interface::program::ID;
pub const STAKE_HISTORY_ID: Pubkey = solana_stake_interface::sysvar::stake_history::ID;
pub const STAKE_CONFIG_ID: Pubkey = pubkey!("StakeConfig11111111111111111111111111111111");
pub const VOTE_PROGRAM_ID: Pubkey = pubkey!("Vote111111111111111111111111111111111111111");
