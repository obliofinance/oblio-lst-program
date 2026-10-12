use anchor_lang::prelude::*;

#[error_code]
pub enum OblioError {
    #[msg("The protocol is paused.")]
    ProtocolPaused,
    #[msg("The exchange rate is stale: run sync_rate in the current epoch first.")]
    RateStale,
    #[msg("Epoch rewards are still being distributed; the rate cannot be synced yet.")]
    EpochRewardsActive,
    #[msg("Amount must be greater than zero.")]
    ZeroAmount,
    #[msg("Deposit is below the configured minimum.")]
    DepositBelowMinimum,
    #[msg("Unstake is below the configured minimum.")]
    UnstakeBelowMinimum,
    #[msg("Deposit would exceed the configured TVL cap.")]
    TvlCapExceeded,
    #[msg("Deposit would exceed the per-validator concentration cap.")]
    BucketCapExceeded,
    #[msg("Slippage tolerance exceeded.")]
    SlippageExceeded,
    #[msg("Resulting share supply would fall below the configured minimum.")]
    SupplyBelowMinimum,
    #[msg("Arithmetic overflow, underflow or division by zero.")]
    MathOverflow,
    #[msg("Gateway accounting is insolvent for the requested operation.")]
    Insolvent,
    #[msg("The instant-exit reserve cannot cover this withdrawal.")]
    ReservePoolDepleted,
    #[msg("Ticket cooldown has not elapsed.")]
    TicketStillLocked,
    #[msg("Ticket is not funded yet; liquidity is still being unstaked.")]
    TicketNotFunded,
    #[msg("Recipient would not be rent exempt after the payout.")]
    RecipientBelowRentExemption,
    #[msg("Remaining accounts must list every bucket exactly once, in index order.")]
    InvalidBucketSet,
    #[msg("Bucket is not in a state that permits this operation.")]
    InvalidBucketStatus,
    #[msg("Bucket weight is invalid for its status.")]
    InvalidWeight,
    #[msg("Maximum number of buckets reached.")]
    MaxBucketsReached,
    #[msg("Bucket primary stake is not delegated.")]
    BucketNotDelegated,
    #[msg("Bucket transient slot is busy; merge it first.")]
    TransientSlotBusy,
    #[msg("Transient stake is not active yet and cannot be merged.")]
    TransientStakeNotActive,
    #[msg("Bucket unstake slot is busy; withdraw it first.")]
    UnstakeSlotBusy,
    #[msg("The unstaking account is not fully inactive yet.")]
    StakeNotInactive,
    #[msg("Chosen bucket is not the most over-weight bucket.")]
    NotMostOverweight,
    #[msg("Nothing to decrease.")]
    NothingToDecrease,
    #[msg("Not enough unallocated liquid funds.")]
    InsufficientUnallocatedFunds,
    #[msg("Nothing to claim.")]
    NothingToClaim,
    #[msg("The provided stake account is not in a depositable state.")]
    InvalidStakeState,
    #[msg("Stake lockup is in force.")]
    StakeLockupInForce,
    #[msg("Protocol share supply is zero.")]
    NoSupply,
    #[msg("Invalid protocol parameters.")]
    InvalidParams,
    #[msg("Timelock has not elapsed.")]
    TimelockNotElapsed,
    #[msg("No pending parameter update.")]
    NoPendingParams,
    #[msg("No pending timelocked action.")]
    NoPendingAction,
    #[msg("Executed action does not match the pending proposal.")]
    ActionMismatch,
    #[msg("Recipient must not be a protocol account.")]
    InvalidRecipient,
    #[msg("Bucket has a transient stake in flight; merge it first.")]
    TransientInFlight,
    #[msg("No pending admin transfer.")]
    NoPendingAdmin,
    #[msg("Caller is not authorized.")]
    Unauthorized,
}
