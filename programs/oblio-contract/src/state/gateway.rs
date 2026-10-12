use anchor_lang::prelude::*;
use crate::constants::BPS_DENOMINATOR;
use crate::errors::OblioError;

fn to_u64(v: u128) -> Result<u64> {
    u64::try_from(v).map_err(|_| error!(OblioError::MathOverflow))
}

/// Operational state. Governance lives in `ProtocolConfig`.
///
/// Accounting model (lamports):
///   gateway.lamports = rent + protocol_fee + (tickets_funded - tickets_claimed) + reserve + free
///   total_assets     = Σ staked value + gateway.lamports - rent - protocol_fee - (tickets_ordered - tickets_claimed)
///   share price      = (total_assets - locked_profit(now) + offset) / (mint.supply + offset)
/// Share supply is NEVER cached: it is always read from the mint.
#[account]
#[derive(InitSpace, Default)]
pub struct GatewayState {
    pub bump: u8,
    pub mint_bump: u8,
    pub mint_authority_bump: u8,
    pub paused: bool,
    /// Set only by the circuit breaker. Blocks every share-price-dependent path, including ticket orders.
    pub breaker_tripped: bool,
    /// Immutable virtual offset (lamports) applied on both sides of the share price.
    pub virtual_offset_lamports: u64,

    pub total_assets: u64,
    pub locked_profit: u64,
    pub lock_start_ts: i64,
    pub lock_duration_secs: i64,
    pub last_synced_epoch: u64,
    /// WAD-scaled rate at the last successful sync; 0 disables the bound check once.
    pub last_rate_wad: u64,

    pub reserve_pool_lamports: u64,
    pub protocol_fee_lamports: u64,

    // FIFO withdrawal queue (cumulative lamports, monotonic).
    pub ticket_queue_ordered: u128,
    pub ticket_queue_funded: u128,
    pub ticket_queue_claimed: u128,

    /// Σ (active + transient) over buckets.
    pub staked_lamports: u64,
    /// Σ unstake_lamports over buckets (liquidity already on its way back).
    pub deactivating_lamports: u64,
    pub bucket_count: u16,
    pub total_weight: u32,

    pub version: u8,
    pub reserved: [u8; 31],
}

impl GatewayState {
    // ---------- profit vesting (locked-profit pattern) ----------

    pub fn locked_profit_at(&self, now: i64) -> u64 {
        if self.locked_profit == 0 || self.lock_duration_secs <= 0 {
            return 0;
        }
        let elapsed = now.saturating_sub(self.lock_start_ts).max(0);
        if elapsed >= self.lock_duration_secs {
            return 0;
        }
        let remaining = (self.lock_duration_secs - elapsed) as u128;
        (self.locked_profit as u128 * remaining / self.lock_duration_secs as u128) as u64
    }

    /// Assets backing shares *right now* (excludes still-vesting rewards).
    pub fn vested_assets(&self, now: i64) -> Result<u64> {
        self.total_assets
            .checked_sub(self.locked_profit_at(now))
            .ok_or_else(|| error!(OblioError::MathOverflow))
    }

    /// Call *after* `total_assets` already includes `amount`.
    pub fn lock_profit(&mut self, amount: u64, now: i64, duration: i64) -> Result<()> {
        if amount == 0 || duration <= 0 {
            return Ok(());
        }
        let new_locked = self
            .locked_profit_at(now)
            .checked_add(amount)
            .ok_or(OblioError::MathOverflow)?;
        self.locked_profit = new_locked.min(self.total_assets);
        self.lock_start_ts = now;
        self.lock_duration_secs = duration;
        Ok(())
    }

    /// Losses are absorbed by unvested profit first.
    pub fn absorb_loss(&mut self, loss: u64, now: i64) {
        self.locked_profit = self.locked_profit_at(now).saturating_sub(loss);
        self.lock_start_ts = now;
    }

    /// (numerator, denominator) of the share price including the virtual offset.
    pub fn rate_parts(&self, supply: u64, now: i64) -> Result<(u128, u128)> {
        let off = self.virtual_offset_lamports as u128;
        Ok((self.vested_assets(now)? as u128 + off, supply as u128 + off))
    }

    // ---------- withdrawal queue / physical solvency ----------

    pub fn tickets_outstanding(&self) -> Result<u64> {
        to_u64(self.ticket_queue_ordered.checked_sub(self.ticket_queue_claimed).ok_or(OblioError::MathOverflow)?)
    }
    pub fn tickets_unfunded(&self) -> Result<u64> {
        to_u64(self.ticket_queue_ordered.checked_sub(self.ticket_queue_funded).ok_or(OblioError::MathOverflow)?)
    }
    pub fn tickets_funded_unclaimed(&self) -> Result<u64> {
        to_u64(self.ticket_queue_funded.checked_sub(self.ticket_queue_claimed).ok_or(OblioError::MathOverflow)?)
    }

    /// Lamports physically earmarked for something other than "free".
    pub fn committed_lamports(&self) -> Result<u64> {
        self.protocol_fee_lamports
            .checked_add(self.tickets_funded_unclaimed()?)
            .and_then(|v| v.checked_add(self.reserve_pool_lamports))
            .ok_or_else(|| error!(OblioError::MathOverflow))
    }

    /// Unallocated lamports. Errors if the physical balance cannot cover every earmark (invariant I1).
    pub fn free_lamports(&self, physical: u64, rent_min: u64) -> Result<u64> {
        let committed = self.committed_lamports()?;
        physical
            .checked_sub(rent_min)
            .and_then(|v| v.checked_sub(committed))
            .ok_or_else(|| error!(OblioError::Insolvent))
    }

    pub fn assert_solvent(&self, physical: u64, rent_min: u64) -> Result<()> {
        self.free_lamports(physical, rent_min).map(|_| ())
    }

    pub fn target_reserve(&self, ratio_bps: u16) -> Result<u64> {
        to_u64(self.total_assets as u128 * ratio_bps as u128 / BPS_DENOMINATOR as u128)
    }

    /// Routes free liquidity: (0) release reserve above target, (1) fund the ticket queue FIFO,
    /// (2) refill the instant-exit reserve. Returns what is left to delegate.
    pub fn settle_liquidity(&mut self, physical: u64, rent_min: u64, reserve_ratio_bps: u16) -> Result<u64> {
        let mut free = self.free_lamports(physical, rent_min)?;
        let target = self.target_reserve(reserve_ratio_bps)?;

        if self.reserve_pool_lamports > target {
            let excess = self.reserve_pool_lamports - target;
            self.reserve_pool_lamports = target;
            free = free.checked_add(excess).ok_or(OblioError::MathOverflow)?;
        }

        let unfunded = self.ticket_queue_ordered - self.ticket_queue_funded;
        let fund = unfunded.min(free as u128);
        self.ticket_queue_funded += fund;
        free -= fund as u64;

        let refill = target.saturating_sub(self.reserve_pool_lamports).min(free);
        self.reserve_pool_lamports += refill;
        free -= refill;
        Ok(free)
    }

    /// Rewards land at the epoch boundary, so any max-age > 0 would let late depositors capture them.
    /// Freshness is therefore a hard invariant, not a tunable.
    pub fn require_fresh(&self, epoch: u64) -> Result<()> {
        require_eq!(epoch, self.last_synced_epoch, OblioError::RateStale);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gw() -> GatewayState {
        GatewayState { total_assets: 1_000, virtual_offset_lamports: 1, ..Default::default() }
    }

    #[test]
    fn profit_vests_linearly_and_never_exceeds_backing() {
        let mut g = gw();
        g.total_assets += 100;
        g.lock_profit(100, 0, 100).unwrap();
        assert_eq!(g.vested_assets(0).unwrap(), 1_000);
        assert_eq!(g.vested_assets(50).unwrap(), 1_050);
        assert_eq!(g.vested_assets(100).unwrap(), 1_100);
        assert_eq!(g.vested_assets(500).unwrap(), 1_100);
    }

    #[test]
    fn settle_funds_queue_before_reserve() {
        let mut g = gw();
        g.ticket_queue_ordered = 300;
        let left = g.settle_liquidity(1_000, 0, 1_000).unwrap(); // reserve target = 100
        assert_eq!(g.ticket_queue_funded, 300);
        assert_eq!(g.reserve_pool_lamports, 100);
        assert_eq!(left, 600);
        g.assert_solvent(1_000, 0).unwrap();
    }
}
