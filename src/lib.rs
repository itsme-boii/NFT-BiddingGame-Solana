use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer as soltransfer, Transfer as SolTransfer};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::metadata::{
    create_master_edition_v3, create_metadata_accounts_v3, CreateMasterEditionV3,
    CreateMetadataAccountsV3, Metadata,
};
use anchor_spl::token::{
    self, mint_to, transfer_checked, Mint, MintTo, Token, TokenAccount, TransferChecked,
};
use mpl_token_metadata::types::{Collection, Creator, DataV2};
use std::collections::HashMap;

declare_id!("FoiwjsfGbN1k1QkWRtZn1KdEtEM6XyRgAXz71KJJk3GU");

#[program]
pub mod nft_stake_game {
    use super::*;

    //Initialising State
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let state = &mut ctx.accounts.state;
        let clock = Clock::get()?;
        let timelock_duration: i64 = 7200 as i64;
        state.owner = ctx.accounts.owner.key();
        state.total_pool = 0;
        state.timelock_deadline = clock.unix_timestamp + timelock_duration;
        state.stake_count = 0;
        state.scores = Vec::new();
        msg!("State initialized:");
        msg!("  Owner: {}", state.owner);
        msg!("  Total Pool: {}", state.total_pool);
        msg!("  Timelock Deadline: {}", state.timelock_deadline);
        msg!("  Stake Count: {}", state.stake_count);
        msg!("  Scores: {:?}", state.scores);

        Ok(())
    }

    //Minting NFT
    pub fn create_single_nft(
        ctx: Context<CreateNFT>,
        input_params: InputParams,
        address: Pubkey,
    ) -> Result<()> {
        // creating MINT To token_account with signer= authority and mint=mint
        let cpi_context = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.token_account.to_account_info(),
                authority: ctx.accounts.signer.to_account_info(),
            },
        );
        mint_to(cpi_context, 1)?;

        // Creating Metadata Account
        msg!("Run create metadata accounts");
        create_metadata_accounts_v3(
            CpiContext::new(
                ctx.accounts.metadata_program.to_account_info(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.nft_metadata.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    mint_authority: ctx.accounts.signer.to_account_info(),
                    update_authority: ctx.accounts.signer.to_account_info(),
                    payer: ctx.accounts.signer.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
            ),
            DataV2 {
                name: input_params.name,
                symbol: input_params.symbol,
                uri: input_params.uri,
                seller_fee_basis_points: 0,
                creators: Some(vec![Creator {
                    address: address,
                    verified: false,
                    share: 100,
                }]),
                collection: None,
                uses: None,
            },
            false,
            true,
            None,
        )?;

        // Creating Master Edition
        msg!("Run create master edition");

        create_master_edition_v3(
            CpiContext::new(
                ctx.accounts.metadata_program.to_account_info(),
                CreateMasterEditionV3 {
                    edition: ctx.accounts.master_edition_account.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    update_authority: ctx.accounts.signer.to_account_info(),
                    mint_authority: ctx.accounts.signer.to_account_info(),
                    payer: ctx.accounts.signer.to_account_info(),
                    metadata: ctx.accounts.nft_metadata.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    token_program: ctx.accounts.token_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
            ),
            Some(1),
        )?;

        msg!("Minted NFT successfully");

        Ok(())
    }

    //Stacking NFT
    pub fn stake_nft(ctx: Context<StakeNft>, entry_fee: u64) -> Result<()> {
        let state = &mut ctx.accounts.state;
        let bidder = &mut ctx.accounts.bidder;
        if !state
            .scores
            .iter()
            .any(|entry| entry.player == bidder.key())
        {
            state.scores.push(ScoreEntry {
                player: bidder.key(),
                score: 0,
            });
            msg!("Added user {} to scores with initial score 0", bidder.key());
        }
        let sol_to_usd_rate: f64 = 0.046;
        let entry_fee_sol: f64 = entry_fee as f64 * sol_to_usd_rate;
        let lamports_required = (entry_fee_sol * 1_000_000_000.0).round() as u64;
        msg!("Calculated entry fee: {} SOL", entry_fee_sol);
        msg!("Required lamports: {}", lamports_required);
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            SolTransfer {
                from: ctx.accounts.bidder.to_account_info(),
                to: ctx.accounts.program_account.to_account_info(),
            },
        );
        soltransfer(cpi_context, lamports_required)?;
        msg!(
            "Transferred {} lamports from bidder to program account",
            lamports_required
        );

        let cpi_context = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.bidder_token_account.to_account_info(),
                to: ctx.accounts.escrow_account.to_account_info(),
                authority: ctx.accounts.bidder.to_account_info(),
                mint: ctx.accounts.nft_mint.to_account_info(),
            },
        );

        transfer_checked(cpi_context, 1, ctx.accounts.nft_mint.decimals)?;
        msg!("Transferred NFT from bidder to escrow account");

        state.total_pool += lamports_required;
        state.stake_count += 1;
        msg!(
            "State updated: total_pool = {}, stake_count = {}",
            state.total_pool,
            state.stake_count
        );
        msg!("User has staked successfully");
        Ok(())
    }

    //Updating Score
    pub fn update_scores(ctx: Context<UpdateScores>, scores: Vec<ScoreEntry>) -> Result<()> {
        let state = &mut ctx.accounts.state;

        // Update scores in the state
        for score_update in scores {
            msg!(
                "Score is {} for user {}",
                score_update.score,
                score_update.player
            );
            if let Some(entry) = state
                .scores
                .iter_mut()
                .find(|entry| entry.player == score_update.player.key())
            {
                // Update the score for the existing user
                entry.score = score_update.score;
                msg!(
                    "Updated score for user {}: {}",
                    score_update.player.key(),
                    score_update.score
                );
            }
        }

        Ok(())
    }

    //Distribute rewards
    pub fn distribute_rewards(ctx: Context<DistributeRewards>) -> Result<()> {
        let state = &mut ctx.accounts.state;
        let owner_share = (state.total_pool as f64 * 0.2) as u64; // 20% to owner
        let mut prize_pool = state.total_pool - owner_share;

        msg!("Total Pool: {}", state.total_pool);

        // Sort scores descending
        let mut scores: Vec<&ScoreEntry> = state.scores.iter().collect();
        scores.sort_by(|a, b| b.score.cmp(&a.score));
        msg!("Owner share: {}", owner_share);
        msg!("Prize pool after owner share: {}", prize_pool);

        let entry_fee: u64 = 10;
        let sol_to_usd_rate: f64 = 0.046;
        let entry_fee_sol: f64 = entry_fee as f64 * sol_to_usd_rate;
        let returned_entry_fee: u64 = (entry_fee_sol * 1_000_000_000.0).round() as u64;
        //Initalising HashMap
        let mut payouts: HashMap<Pubkey, u64> = HashMap::new();
        msg!("Sorted scores: {:?}", scores);

        let mut total_entry_fees_refunded: u64 = 0;

        if let Some(top1) = scores.get(0) {
            payouts.insert(top1.player, returned_entry_fee);
            prize_pool = prize_pool.saturating_sub(returned_entry_fee);
            total_entry_fees_refunded += returned_entry_fee;
        }
        if let Some(top2) = scores.get(1) {
            payouts.insert(top2.player, returned_entry_fee);
            prize_pool = prize_pool.saturating_sub(returned_entry_fee);
            total_entry_fees_refunded += returned_entry_fee;
        }
        if let Some(top3) = scores.get(2) {
            payouts.insert(top3.player, returned_entry_fee);
            prize_pool = prize_pool.saturating_sub(returned_entry_fee);
            total_entry_fees_refunded += returned_entry_fee;
        }

        msg!("Total entry fees refunded: {}", total_entry_fees_refunded);
        msg!("Prize pool after refunding entry fees: {}", prize_pool);
        // Allocate rewards for top 3

        //25% to Top1
        if let Some(top1) = scores.get(0) {
            let top1_reward = (prize_pool as f64 * 0.25) as u64;
            *payouts.entry(top1.player).or_insert(0) += top1_reward;
            msg!("Top 1: {} - Reward: {}", top1.player, top1_reward);
        }

        //20% to Top2
        if let Some(top2) = scores.get(1) {
            let top2_reward = (prize_pool as f64 * 0.2) as u64;
            *payouts.entry(top2.player).or_insert(0) += top2_reward;
            msg!("Top 2: {} - Reward: {}", top2.player, top2_reward);
        }

        //15% to Top3
        if let Some(top3) = scores.get(2) {
            let top3_reward = (prize_pool as f64 * 0.15) as u64;
            *payouts.entry(top3.player).or_insert(0) += top3_reward;
            msg!("Top 3: {} - Reward: {}", top3.player, top3_reward);
        }

        // Remaining pool for others
        let distributed_to_top3: u64 = payouts.values().sum();
        let remaining_pool = prize_pool - distributed_to_top3;
        let other_count = (scores.len() as u64).saturating_sub(3);
        let mut remaining_rewards = remaining_pool as f64;

        msg!("Distributed to top 3: {}", distributed_to_top3);
        msg!("Remaining pool for other participants: {}", remaining_pool);
        msg!("Number of other participants: {}", other_count);
        if other_count > 0 {
            for (i, score) in scores.iter().skip(3).enumerate() {
                let rank_factor = 1.0 / (i as f64 + 4.0);
                let user_share = (remaining_rewards * rank_factor).round() as u64;

                payouts.insert(score.player, user_share);
                remaining_rewards -= user_share as f64;

                msg!("Rank {}: {} - Share: {}", i + 4, score.player, user_share);
            }
        }

        let remaining_to_owner = remaining_rewards.round() as u64;
        payouts.entry(state.owner).or_insert(0);
        *payouts.get_mut(&state.owner).unwrap() += remaining_to_owner;

        msg!("Remaining rewards added to owner: {}", remaining_to_owner);

        msg!("Final payout distribution: {:?}", payouts);

        msg!("Rewards distributed successfully");

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account( 
        init, 
        payer = owner, 
        space = 8 + 32 + 8 + 8 + (4 + 50 * (32 + 8)) + 8,
         seeds = [b"state"],
         bump)]
    pub state: Account<'info, State>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StakeNft<'info> {
    #[account(
        mut,
        seeds = [b"state"],
        bump
    )]
    pub state: Account<'info, State>,
    #[account(mut)]
    pub bidder: Signer<'info>,
    #[account(mut)]
    pub nft_mint: Account<'info, Mint>,
    #[account(mut)]
    pub bidder_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub program_account: SystemAccount<'info>,
    #[account(
    init_if_needed,
    payer=bidder,
    seeds=[
        b"escrow_account".as_ref(),
        nft_mint.key().as_ref(),
    ],
    bump,
    token::mint = nft_mint,
    token::authority = program_authority
    )]
    pub escrow_account: Account<'info, TokenAccount>,
    #[account(
        seeds = [
            b"escrow_authority"],
        bump,
    )]
    pub program_authority: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateScores<'info> {
    #[account(mut, has_one = owner)]
    pub state: Account<'info, State>,
    pub owner: Signer<'info>,
}

#[derive(Accounts)]
pub struct DistributeRewards<'info> {
    #[account(mut)]
    pub state: Account<'info, State>,
    #[account(mut)]
    pub owner_account: SystemAccount<'info>,
}

#[account]
pub struct State {
    pub owner: Pubkey,
    pub total_pool: u64,
    pub stake_count: u64,
    pub scores: Vec<ScoreEntry>,
    pub timelock_deadline: i64,
}
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ScoreEntry {
    pub player: Pubkey,
    pub score: u64,
}

#[error_code]
pub enum CustomError {
    #[msg("Invalid entry fee. Must be 10 USDC.")]
    InvalidEntryFee,
}

#[derive(Accounts)]
#[instruction(input_params : InputParams)]
pub struct CreateNFT<'info> {
    #[account(mut, signer)]
    pub signer: Signer<'info>,
    #[account(
        init,
        payer = signer,
        mint::decimals = 0,
        mint::authority = signer.key(),
        mint::freeze_authority = signer.key(),
        seeds=[
            b"mint".as_ref(),
            input_params.name.as_bytes()
        ],
        bump
    )]
    pub mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer,
    )]
    pub token_account: Account<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub metadata_program: Program<'info, Metadata>,
    #[account(
        mut,
        seeds = [
            b"metadata".as_ref(),
            metadata_program.key().as_ref(),
            mint.key().as_ref(),
            b"edition".as_ref(),
        ],
        bump,
        seeds::program = metadata_program.key()
    )]
    pub master_edition_account: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [
            b"metadata".as_ref(),
            metadata_program.key().as_ref(),
            mint.key().as_ref(),
        ],
        bump,
        seeds::program = metadata_program.key()
    )]
    /// CHECK:
    pub nft_metadata: UncheckedAccount<'info>,
}

#[account]
pub struct InputParams {
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub collection_name: String,
}
