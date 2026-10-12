use anchor_lang::{prelude::*, solana_program};
use solana_stake_interface::instruction::delegate_stake;
use crate::state::{gateway::GatewayState, bucket::BucketRegistry};
use crate::constants::*;
use crate::errors::OblioError;
use crate::events::BucketsRebalancedEvent;

#[derive(Accounts)]
pub struct RebalanceAndDelegate<'info> {
    #[account(
        mut,
        seeds = [GATEWAY_SEED],
        bump = gateway.bump,
        constraint = !gateway.is_paused @ OblioError::ProtocolPaused
    )]
    pub gateway: Box<Account<'info, GatewayState>>,

    #[account(
        mut,
        seeds = [BUCKET_REGISTRY_SEED],
        bump = registry.bump
    )]
    pub registry: Box<Account<'info, BucketRegistry>>,

    pub clock: Sysvar<'info, Clock>,
    pub rent: Sysvar<'info, Rent>,

    /// CHECK: Stake History Sysvar
    #[account(address = solana_stake_interface::sysvar::stake_history::ID @ OblioError::InvalidSysvar)]
    pub stake_history: UncheckedAccount<'info>,

    /// CHECK: Native Stake Config Account
    #[account(address = STAKE_CONFIG_ID @ OblioError::InvalidSysvar)]
    pub stake_config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: Solana Native Stake Program
    #[account(address = solana_stake_interface::program::ID @ OblioError::InvalidStakeProgram)]
    pub stake_program: UncheckedAccount<'info>,

    /// Anyone can permissionlessly execute delegation of available funds
    pub crank_signer: Signer<'info>,
}

pub fn rebalance_and_delegate_handler<'info>(
    ctx: Context<'info, RebalanceAndDelegate<'info>>,
) -> Result<()> {
    let gateway = &mut ctx.accounts.gateway;

    let gateway_info = gateway.to_account_info();
    let min_rent = Rent::get()?.minimum_balance(gateway_info.data_len());
    let gateway_lamports = gateway_info.lamports();

    // Protect all earmarked balances
    let reserved_funds = min_rent
        .checked_add(gateway.subvention_pool_lamports).ok_or(OblioError::MathOverflow)?
        .checked_add(gateway.protocol_fee_lamports).ok_or(OblioError::MathOverflow)?
        .checked_add(gateway.pending_ticket_lamports).ok_or(OblioError::MathOverflow)?
        .checked_add(gateway.reserve_pool_lamports).ok_or(OblioError::MathOverflow)?;

    let mut available_to_delegate = gateway_lamports.saturating_sub(reserved_funds);
    require!(available_to_delegate >= MIN_DELEGATION_LAMPORTS, OblioError::InsufficientUnallocatedFunds);

    let total_liquid_available = available_to_delegate;
    let gateway_seeds: &[&[&[u8]]] = &[&[GATEWAY_SEED, &[gateway.bump]]];
    let stake_space = std::mem::size_of::<solana_stake_interface::state::StakeStateV2>();
    let stake_rent = ctx.accounts.rent.minimum_balance(stake_space);

    for i in 0..BUCKET_COUNT {
        let bucket = ctx.accounts.registry.buckets[i];
        if bucket.is_whitelisted && bucket.weight_bps > 0 {
            let target_additional = ((total_liquid_available as u128 * bucket.weight_bps as u128) / 10_000) as u64;
            let transfer_amount = target_additional.min(available_to_delegate);

            if transfer_amount >= MIN_DELEGATION_LAMPORTS {
                let is_first_delegation = bucket.active_lamports == 0;

                if is_first_delegation {
                    // Initial delegation targets the primary stake account PDA
                    if let (Some(stake_info), Some(vote_info)) = (
                        ctx.remaining_accounts.iter().find(|a| a.key == &bucket.stake_account),
                        ctx.remaining_accounts.iter().find(|a| a.key == &bucket.vote_account),
                    ) {
                        require!(stake_info.is_writable, OblioError::Unauthorized);

                        **gateway_info.try_borrow_mut_lamports()? -= transfer_amount;
                        **stake_info.try_borrow_mut_lamports()? += transfer_amount;

                        let registry = &mut ctx.accounts.registry;
                        registry.buckets[i].active_lamports = registry.buckets[i].active_lamports
                            .checked_add(transfer_amount)
                            .ok_or(OblioError::MathOverflow)?;

                        registry.total_delegated_lamports = registry.total_delegated_lamports
                            .checked_add(transfer_amount)
                            .ok_or(OblioError::MathOverflow)?;

                        available_to_delegate = available_to_delegate.saturating_sub(transfer_amount);

                        let ix_delegate = delegate_stake(
                            &bucket.stake_account,
                            &gateway.key(),
                            &bucket.vote_account,
                        );

                        solana_program::program::invoke_signed(
                            &ix_delegate,
                            &[
                                stake_info.clone(),
                                vote_info.clone(),
                                ctx.accounts.clock.to_account_info(),
                                ctx.accounts.stake_history.to_account_info(),
                                ctx.accounts.stake_config.to_account_info(),
                                gateway_info.clone(),
                                ctx.accounts.stake_program.to_account_info(),
                            ],
                            gateway_seeds,
                        )?;
                    }
                } else if bucket.transient_lamports == 0 {
                    // Top-up delegation targets dynamic transient stake PDA
                    let bucket_index_u8 = i as u8;
                    let (transient_pda, transient_bump) = Pubkey::find_program_address(
                        &[TRANSIENT_STAKE_SEED, &[bucket_index_u8], &bucket.transient_seed.to_le_bytes()],
                        ctx.program_id,
                    );

                    if let (Some(transient_info), Some(vote_info)) = (
                        ctx.remaining_accounts.iter().find(|a| a.key == &transient_pda),
                        ctx.remaining_accounts.iter().find(|a| a.key == &bucket.vote_account),
                    ) {
                        require!(transient_info.is_writable, OblioError::Unauthorized);
                        require!(transfer_amount > stake_rent, OblioError::DepositBelowMinimum);

                        let transient_signer_seeds: &[&[&[u8]]] = &[&[
                            TRANSIENT_STAKE_SEED,
                            &[bucket_index_u8],
                            &bucket.transient_seed.to_le_bytes(),
                            &[transient_bump],
                        ]];

                        // 1. Move lamports directly from program-owned Gateway to transient account
                        **gateway_info.try_borrow_mut_lamports()? -= transfer_amount;
                        **transient_info.try_borrow_mut_lamports()? += transfer_amount;

                        // 2. Allocate account space for StakeStateV2
                        let ix_allocate = anchor_lang::solana_program::system_instruction::allocate(
                            &transient_pda,
                            stake_space as u64,
                        );
                        solana_program::program::invoke_signed(
                            &ix_allocate,
                            &[transient_info.clone(), ctx.accounts.system_program.to_account_info()],
                            transient_signer_seeds,
                        )?;

                        // 3. Assign ownership to Native Stake Program
                        let ix_assign = anchor_lang::solana_program::system_instruction::assign(
                            &transient_pda,
                            &solana_stake_interface::program::ID,
                        );
                        solana_program::program::invoke_signed(
                            &ix_assign,
                            &[transient_info.clone(), ctx.accounts.system_program.to_account_info()],
                            transient_signer_seeds,
                        )?;

                        // 4. Initialize StakeStateV2
                        let authorized = solana_stake_interface::state::Authorized {
                            staker: gateway.key(),
                            withdrawer: gateway.key(),
                        };

                        let ix_init = solana_stake_interface::instruction::initialize(
                            &transient_pda,
                            &authorized,
                            &solana_stake_interface::state::Lockup::default(),
                        );
                        solana_program::program::invoke(
                            &ix_init,
                            &[
                                transient_info.clone(),
                                ctx.accounts.rent.to_account_info(),
                                ctx.accounts.stake_program.to_account_info(),
                            ],
                        )?;

                        // 5. Delegate transient stake to validator
                        let ix_delegate = delegate_stake(
                            &transient_pda,
                            &gateway.key(),
                            &bucket.vote_account,
                        );
                        solana_program::program::invoke_signed(
                            &ix_delegate,
                            &[
                                transient_info.clone(),
                                vote_info.clone(),
                                ctx.accounts.clock.to_account_info(),
                                ctx.accounts.stake_history.to_account_info(),
                                ctx.accounts.stake_config.to_account_info(),
                                gateway_info.clone(),
                                ctx.accounts.stake_program.to_account_info(),
                            ],
                            gateway_seeds,
                        )?;

                        let registry = &mut ctx.accounts.registry;
                        registry.buckets[i].transient_account = transient_pda;
                        registry.buckets[i].transient_bump = transient_bump;
                        registry.buckets[i].transient_lamports = transfer_amount;
                        registry.total_delegated_lamports = registry.total_delegated_lamports
                            .checked_add(transfer_amount)
                            .ok_or(OblioError::MathOverflow)?;

                        available_to_delegate = available_to_delegate.saturating_sub(transfer_amount);
                    }
                }
            }
        }
    }

    let total_delegated = ctx.accounts.registry.total_delegated_lamports;
    emit!(BucketsRebalancedEvent {
        total_delegated_lamports: total_delegated,
        total_allocated: total_delegated,
    });

    Ok(())
}