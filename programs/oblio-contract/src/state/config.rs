use anchor_lang::prelude::*;
use crate::constants::*;
use crate::errors::OblioError;

/// Every tunable of the protocol. Changed only through the timelocked
/// `propose_params` -> `execute_params` flow.
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolParams {
    /// Target size of the instant-exit reserve, as bps of total assets.
    pub reserve_ratio_bps: u16,
    /// Instant-exit fee floor / ceiling (bps) and curve exponent (1 = linear, 2 = quadratic, ...).
    pub fee_min_bps: u16,
    pub fee_max_bps: u16,
    pub fee_curve_exponent: u8,
    /// Share of the instant-exit fee sent to the treasury; the rest stays in the pool for holders.
    pub protocol_fee_share_bps: u16,
    pub min_deposit_lamports: u64,
    pub min_unstake_lamports: u64,
    /// Minimum delegation enforced by the native stake program.
    pub min_delegation_lamports: u64,
    /// Share supply may never end between 1 and this value (anti-dust-inflation).
    pub min_total_shares: u64,
    pub min_reward_injection_lamports: u64,
    pub tvl_cap_lamports: u64,
    pub max_buckets: u16,
    /// Per-validator concentration cap (bps of total assets).
    pub max_bucket_share_bps: u16,
    /// Deviation from target weight tolerated before `decrease_stake` may rebalance (bps of staked).
    pub rebalance_tolerance_bps: u16,
    pub ticket_cooldown_epochs: u64,
    /// Linear vesting window for realised rewards / donations.
    pub profit_vesting_secs: i64,
    /// Circuit-breaker bounds on the exchange rate between two syncs (bps).
    pub rate_drop_tolerance_bps: u16,
    pub max_rate_jump_bps: u16,
    pub timelock_secs: i64,
}

impl ProtocolParams {
    pub fn validate(&self) -> Result<()> {
        let bps = BPS_DENOMINATOR as u16;
        require!(self.reserve_ratio_bps <= bps, OblioError::InvalidParams);
        require!(self.fee_min_bps <= self.fee_max_bps && self.fee_max_bps <= bps, OblioError::InvalidParams);
        require!(
            (1..=MAX_FEE_CURVE_EXPONENT).contains(&self.fee_curve_exponent),
            OblioError::InvalidParams
        );
        require!(self.protocol_fee_share_bps <= bps, OblioError::InvalidParams);
        require!(self.min_deposit_lamports > 0 && self.min_unstake_lamports > 0, OblioError::InvalidParams);
        require!(self.min_delegation_lamports > 0, OblioError::InvalidParams);
        // Anything below the native minimum delegation would force sub-minimum stake operations.
        require!(self.min_unstake_lamports >= self.min_delegation_lamports, OblioError::InvalidParams);
        require!(self.min_deposit_lamports >= self.min_delegation_lamports, OblioError::InvalidParams);
        require!(self.tvl_cap_lamports > 0, OblioError::InvalidParams);
        require!(self.max_buckets > 0, OblioError::InvalidParams);
        require!((1..=bps).contains(&self.max_bucket_share_bps), OblioError::InvalidParams);
        require!(self.rebalance_tolerance_bps <= bps, OblioError::InvalidParams);
        require!((0..=MAX_VESTING_SECS).contains(&self.profit_vesting_secs), OblioError::InvalidParams);
        require!(self.rate_drop_tolerance_bps <= bps, OblioError::InvalidParams);
        require!((1..=bps).contains(&self.max_rate_jump_bps), OblioError::InvalidParams);
        require!((MIN_TIMELOCK_SECS..=MAX_TIMELOCK_SECS).contains(&self.timelock_secs), OblioError::InvalidParams);
        Ok(())
    }
}

/// Admin actions that must sit behind the timelock (propose -> wait -> call the normal instruction).
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelockedAction {
    SetGuardian { guardian: Pubkey },
    SetFeeRecipient { recipient: Pubkey },
    UpdateConfidentialMint { auto_approve: bool, auditor_elgamal_pubkey: Option<[u8; 32]> },
    UpdateBucket { bucket: Pubkey, weight_bps: u16, status: crate::state::BucketStatus },
}

#[account]
#[derive(InitSpace)]
pub struct ProtocolConfig {
    pub admin: Pubkey,
    pub pending_admin: Option<Pubkey>,
    /// Can only pause. Unpausing is admin-only.
    pub guardian: Pubkey,
    pub fee_recipient: Pubkey,
    pub params: ProtocolParams,
    pub pending_params: Option<ProtocolParams>,
    pub pending_params_eta: i64,
    pub pending_action: Option<TimelockedAction>,
    pub pending_action_eta: i64,
    pub version: u8,
    pub bump: u8,
    pub reserved: [u8; 64],
}

impl ProtocolConfig {
    /// Consumes the pending action iff it equals `expected` and its delay has elapsed.
    pub fn consume_action(&mut self, now: i64, expected: &TimelockedAction) -> Result<()> {
        let pending = self.pending_action.ok_or(OblioError::NoPendingAction)?;
        require!(pending == *expected, OblioError::ActionMismatch);
        require_gte!(now, self.pending_action_eta, OblioError::TimelockNotElapsed);
        self.pending_action = None;
        self.pending_action_eta = 0;
        Ok(())
    }
}
