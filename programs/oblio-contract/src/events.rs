use anchor_lang::prelude::*;
use crate::state::{BucketStatus, MintMode, ProtocolParams};

#[event]
pub struct ProtocolInitializedEvent { pub admin: Pubkey, pub mint: Pubkey, pub params: ProtocolParams }
#[event]
pub struct ParamsProposedEvent { pub params: ProtocolParams, pub executable_at: i64 }
#[event]
pub struct ParamsExecutedEvent { pub params: ProtocolParams }
#[event]
pub struct ParamsCancelledEvent { pub admin: Pubkey }
#[event]
pub struct AdminTransferProposedEvent { pub current_admin: Pubkey, pub pending_admin: Pubkey }
#[event]
pub struct AdminTransferAcceptedEvent { pub old_admin: Pubkey, pub new_admin: Pubkey }
#[event]
pub struct AdminTransferCancelledEvent { pub admin: Pubkey }
#[event]
pub struct RoleUpdatedEvent { pub role: u8, pub old: Pubkey, pub new: Pubkey }
#[event]
pub struct PauseChangedEvent { pub paused: bool, pub by: Pubkey, pub timestamp: i64 }
#[event]
pub struct CircuitBreakerTrippedEvent { pub reason: u8, pub old_rate_wad: u64, pub new_rate_wad: u64, pub total_assets: u64, pub real_assets: u64 }
#[event]
pub struct BucketRegisteredEvent { pub index: u16, pub bucket: Pubkey, pub vote_account: Pubkey }
#[event]
pub struct BucketUpdatedEvent { pub index: u16, pub weight_bps: u16, pub status: BucketStatus }
#[event]
pub struct ConfidentialMintUpdatedEvent { pub auto_approve: bool, pub has_auditor: bool }
#[event]
pub struct RateSyncedEvent { pub epoch: u64, pub total_assets: u64, pub locked_profit: u64, pub supply: u64, pub rate_wad: u64, pub multiplier: f64, pub profit: u64 }
#[event]
pub struct DepositDirectEvent { pub depositor: Pubkey, pub lamports: u64, pub shares_minted: u64, pub mode: MintMode }
#[event]
pub struct DepositStakeEvent { pub depositor: Pubkey, pub stake_account: Pubkey, pub bucket: Pubkey, pub lamports: u64, pub shares_minted: u64, pub mode: MintMode }
#[event]
pub struct InstantUnstakeEvent { pub user: Pubkey, pub recipient: Pubkey, pub shares_burned: u64, pub gross_lamports: u64, pub fee_lamports: u64, pub protocol_fee_lamports: u64, pub net_lamports: u64 }
#[event]
pub struct TicketOrderedEvent { pub orderer: Pubkey, pub recipient: Pubkey, pub ticket_id: u64, pub shares_burned: u64, pub sol_claimable: u64, pub queue_end: u128, pub creation_epoch: u64 }
#[event]
pub struct TicketClaimedEvent { pub recipient: Pubkey, pub ticket: Pubkey, pub lamports_paid: u64 }
#[event]
pub struct BucketDelegatedEvent { pub index: u16, pub lamports: u64, pub transient: bool }
#[event]
pub struct TransientMergedEvent { pub index: u16, pub lamports: u64 }
#[event]
pub struct StakeDecreasedEvent { pub index: u16, pub lamports: u64, pub in_place: bool }
#[event]
pub struct InactiveStakeWithdrawnEvent { pub index: u16, pub lamports: u64 }
#[event]
pub struct LiquiditySettledEvent { pub tickets_funded: u128, pub reserve: u64, pub free: u64 }
#[event]
pub struct RewardsInjectedEvent { pub caller: Pubkey, pub lamports: u64 }
#[event]
pub struct ProtocolFeesClaimedEvent { pub recipient: Pubkey, pub lamports: u64 }
#[event]
pub struct ActionProposedEvent { pub executable_at: i64 }
#[event]
pub struct ActionCancelledEvent { pub admin: Pubkey }
#[event]
pub struct BucketDrainedEvent { pub index: u16, pub by: Pubkey }
