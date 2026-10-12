#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod math;
pub mod state;
pub mod utils;

pub use constants::*;
pub use errors::*;
pub use events::*;
pub use instructions::*;
pub use math::*;
pub use state::*;
pub use utils::*;

declare_id!("6oCMmWL81taqYQNxtw8xF5n59FjxhmzweLg8Xaymjxep");

#[program]
pub mod oblio_contract {
    use super::*;

    // ===================== governance =====================

    pub fn initialize_protocol(ctx: Context<InitializeProtocol>, args: InitializeArgs) -> Result<()> {
        initialize_protocol_handler(ctx, args)
    }
    pub fn propose_params(ctx: Context<ProposeParams>, params: ProtocolParams) -> Result<()> {
        propose_params_handler(ctx, params)
    }
    pub fn propose_action(ctx: Context<ProposeAction>, action: TimelockedAction) -> Result<()> {
        propose_action_handler(ctx, action)
    }
    pub fn cancel_action(ctx: Context<ProposeAction>) -> Result<()> {
        cancel_action_handler(ctx)
    }
    pub fn drain_bucket(ctx: Context<DrainBucket>) -> Result<()> {
        drain_bucket_handler(ctx)
    }
    pub fn execute_params(ctx: Context<ExecuteParams>) -> Result<()> {
        execute_params_handler(ctx)
    }
    pub fn cancel_params(ctx: Context<CancelParams>) -> Result<()> {
        cancel_params_handler(ctx)
    }
    pub fn transfer_admin(ctx: Context<AdminOnly>, new_admin: Pubkey) -> Result<()> {
        transfer_admin_handler(ctx, new_admin)
    }
    pub fn cancel_admin_transfer(ctx: Context<AdminOnly>) -> Result<()> {
        cancel_admin_transfer_handler(ctx)
    }
    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        accept_admin_handler(ctx)
    }
    pub fn set_guardian(ctx: Context<AdminOnly>, guardian: Pubkey) -> Result<()> {
        set_guardian_handler(ctx, guardian)
    }
    pub fn set_fee_recipient(ctx: Context<AdminOnly>, recipient: Pubkey) -> Result<()> {
        set_fee_recipient_handler(ctx, recipient)
    }
    pub fn pause(ctx: Context<Pause>) -> Result<()> {
        pause_handler(ctx)
    }
    pub fn unpause(ctx: Context<Unpause>) -> Result<()> {
        unpause_handler(ctx)
    }
    pub fn register_bucket(ctx: Context<RegisterBucket>) -> Result<()> {
        register_bucket_handler(ctx)
    }
    pub fn update_bucket(ctx: Context<UpdateBucket>, weight_bps: u16, status: BucketStatus) -> Result<()> {
        update_bucket_handler(ctx, weight_bps, status)
    }
    pub fn update_confidential_mint(
        ctx: Context<UpdateConfidentialMint>,
        auto_approve: bool,
        auditor_elgamal_pubkey: Option<[u8; 32]>,
    ) -> Result<()> {
        update_confidential_mint_handler(ctx, auto_approve, auditor_elgamal_pubkey)
    }

    // ===================== permissionless cranks =====================

    pub fn sync_rate<'info>(ctx: Context<'info, SyncRate<'info>>) -> Result<()> {
        sync_rate_handler(ctx)
    }
    pub fn settle_liquidity(ctx: Context<SettleLiquidity>) -> Result<()> {
        settle_liquidity_handler(ctx)
    }
    pub fn delegate_to_bucket(ctx: Context<DelegateToBucket>) -> Result<()> {
        delegate_to_bucket_handler(ctx)
    }
    pub fn merge_transient_stake(ctx: Context<MergeTransientStake>) -> Result<()> {
        merge_transient_stake_handler(ctx)
    }
    pub fn decrease_stake<'info>(ctx: Context<'info, DecreaseStake<'info>>) -> Result<()> {
        decrease_stake_handler(ctx)
    }
    pub fn withdraw_inactive_stake(ctx: Context<WithdrawInactiveStake>) -> Result<()> {
        withdraw_inactive_stake_handler(ctx)
    }
    pub fn inject_product_rewards(ctx: Context<InjectProductRewards>, lamports: u64) -> Result<()> {
        inject_product_rewards_handler(ctx, lamports)
    }
    pub fn claim_protocol_fees(ctx: Context<ClaimProtocolFees>) -> Result<()> {
        claim_protocol_fees_handler(ctx)
    }

    // ===================== users =====================

    pub fn deposit_direct(ctx: Context<DepositDirect>, lamports: u64, mode: MintMode, min_shares_out: u64) -> Result<()> {
        deposit_direct_handler(ctx, lamports, mode, min_shares_out)
    }
    pub fn deposit_stake(ctx: Context<DepositStake>, mode: MintMode, min_shares_out: u64) -> Result<()> {
        deposit_stake_handler(ctx, mode, min_shares_out)
    }
    pub fn instant_unstake(ctx: Context<InstantUnstake>, shares_to_burn: u64, min_net_lamports: u64) -> Result<()> {
        instant_unstake_handler(ctx, shares_to_burn, min_net_lamports)
    }
    pub fn order_unstake_ticket(
        ctx: Context<OrderUnstakeTicket>,
        ticket_id: u64,
        shares_to_burn: u64,
        recipient: Pubkey,
        min_sol_claimable: u64,
    ) -> Result<()> {
        order_unstake_ticket_handler(ctx, ticket_id, shares_to_burn, recipient, min_sol_claimable)
    }
    pub fn claim_unstake_ticket(ctx: Context<ClaimUnstakeTicket>, ticket_id: u64) -> Result<()> {
        claim_unstake_ticket_handler(ctx, ticket_id)
    }
}
