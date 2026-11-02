//!
//!

pub mod consts;
pub mod math;

use ahash::HashSet;
use async_trait::async_trait;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use solana_sysvar::clock::{self, Clock};
use spl_associated_token_account::get_associated_token_address;

use crate::{
    account_caching::AccountsCache,
    trading_venue::{
        AddressLookupTableTrait, FromAccount, QuoteRequest, QuoteResult, SwapType, TradingVenue,
        error::TradingVenueError,
        protocol::PoolProtocol,
        token_info::{TOKEN_PROGRAM_ID, TokenInfo},
        venue_creation::{ParsedInstruction, PoolCreation},
    },
};

use consts::{
    BAN_SEED, CREATE_POOL_DISCRIMINATOR, FEE_TIER_SIZE, HARD_ZONE_TARGET_MULTIPLIER,
    JITO_VOTE_ACCOUNT, KIPSELI_PROGRAM_ID, MAX_FEE_TIERS, MAX_PPM, MAX_TITAN_FLOW_TYPES,
    POOL_DISCRIMINATOR, SPL_TOKEN_AMOUNT_OFFSET, SWAP_V3_DISCRIMINATOR, create_pool_accounts,
    pool_offsets,
};
use math::{Curve, FeeTier, insufficient_liquidity, overflow};

const SYSTEM_PROGRAM_ID: Pubkey = Pubkey::from_str_const("11111111111111111111111111111111");
const INSTRUCTIONS_SYSVAR_ID: Pubkey =
    Pubkey::from_str_const("Sysvar1nstructions1111111111111111111111111");

pub fn parse_pool_creations(instructions: &[ParsedInstruction]) -> Vec<PoolCreation> {
    instructions
        .iter()
        .filter(|ix| ix.program_id == KIPSELI_PROGRAM_ID)
        .filter(|ix| ix.data.starts_with(&CREATE_POOL_DISCRIMINATOR))
        .filter_map(|ix| {
            let pool = *ix.accounts.get(create_pool_accounts::POOL)?;
            let base_mint = *ix.accounts.get(create_pool_accounts::BASE_TOKEN)?;
            let quote_mint = *ix.accounts.get(create_pool_accounts::QUOTE_TOKEN)?;
            Some(PoolCreation {
                protocol: PoolProtocol::KipseliPropAmm,
                pool,
                mints: vec![base_mint, quote_mint],
            })
        })
        .collect()
}

#[derive(Clone, Copy, Default, Debug)]
struct PoolState {
    raw_price: u64,
    price_expired_at: i64,
    fee_in_ppm: u64,
    is_active: u8,
    fee_tiers_count: u8,
    max_slot_gap: u8,
    base_fee_tiers: [FeeTier; MAX_FEE_TIERS],
    quote_fee_tiers: [FeeTier; MAX_FEE_TIERS],
    curve: Curve,
    latest_block_slot: u64,
    slot_gap_fee_in_ppm: u64,
    slot_gap_fee_growth_base: u64,
    latest_trade_slot: u64,
    base_amount_in_volume: u64,
    quote_amount_in_volume: u64,
    multiply_fee_amount_in_volume: u64,
    titan_flow_fee_ppm: [u64; MAX_TITAN_FLOW_TYPES],
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, TradingVenueError> {
    data.get(offset..offset + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(TradingVenueError::DeserializationFailed("pool account too short".into()))
}

fn read_i64(data: &[u8], offset: usize) -> Result<i64, TradingVenueError> {
    read_u64(data, offset).map(|v| v as i64)
}

fn read_pubkey(data: &[u8], offset: usize) -> Result<Pubkey, TradingVenueError> {
    data.get(offset..offset + 32)
        .and_then(|s| <[u8; 32]>::try_from(s).ok())
        .map(Pubkey::new_from_array)
        .ok_or(TradingVenueError::DeserializationFailed("pool account too short".into()))
}

fn read_fee_tiers(data: &[u8], offset: usize) -> Result<[FeeTier; MAX_FEE_TIERS], TradingVenueError> {
    let mut tiers = [FeeTier::default(); MAX_FEE_TIERS];
    for (i, tier) in tiers.iter_mut().enumerate() {
        let base = offset + i * FEE_TIER_SIZE;
        *tier = FeeTier {
            qty: read_u64(data, base)?,
            more_fee_ppm: read_u64(data, base + 8)?,
        };
    }
    Ok(tiers)
}

fn read_titan_flow_fees(data: &[u8]) -> Result<[u64; MAX_TITAN_FLOW_TYPES], TradingVenueError> {
    let mut fees = [0u64; MAX_TITAN_FLOW_TYPES];
    for (i, fee) in fees.iter_mut().enumerate() {
        *fee = read_u64(data, pool_offsets::TITAN_FLOW_FEE_PPM + i * 8)?;
    }
    Ok(fees)
}

fn check_pool_account(pubkey: &Pubkey, account: &Account) -> Result<(), TradingVenueError> {
    if account.owner != KIPSELI_PROGRAM_ID
        || account.data.len() < pool_offsets::MIN_LEN
        || !account.data.starts_with(&POOL_DISCRIMINATOR)
    {
        return Err(TradingVenueError::FromAccountError(pubkey.into()));
    }
    Ok(())
}

fn parse_pool_state(data: &[u8]) -> Result<PoolState, TradingVenueError> {
    Ok(PoolState {
        raw_price: read_u64(data, pool_offsets::RAW_PRICE)?,
        price_expired_at: read_i64(data, pool_offsets::PRICE_EXPIRED_AT)?,
        fee_in_ppm: read_u64(data, pool_offsets::FEE_IN_PPM)?,
        is_active: data[pool_offsets::IS_ACTIVE],
        fee_tiers_count: data[pool_offsets::FEE_TIERS_COUNT],
        max_slot_gap: data[pool_offsets::MAX_SLOT_GAP],
        base_fee_tiers: read_fee_tiers(data, pool_offsets::BASE_FEE_TIERS)?,
        quote_fee_tiers: read_fee_tiers(data, pool_offsets::QUOTE_FEE_TIERS)?,
        curve: Curve {
            a: read_i64(data, pool_offsets::A)?,
            b: read_i64(data, pool_offsets::B)?,
            c: read_i64(data, pool_offsets::C)?,
            target_balance: read_u64(data, pool_offsets::TARGET_BALANCE)?,
            bad_penalty_in_ppm: read_u64(data, pool_offsets::BAD_PENALTY_IN_PPM)?,
            bad_curve_multiplier_ppm: read_u64(data, pool_offsets::BAD_CURVE_MULTIPLIER_PPM)?,
            min_slippage_in_ppm: read_i64(data, pool_offsets::MIN_SLIPPAGE_IN_PPM)?,
        },
        latest_block_slot: read_u64(data, pool_offsets::LATEST_BLOCK_SLOT)?,
        slot_gap_fee_in_ppm: read_u64(data, pool_offsets::SLOT_GAP_FEE_IN_PPM)?,
        slot_gap_fee_growth_base: read_u64(data, pool_offsets::SLOT_GAP_FEE_GROWTH_BASE)?,
        latest_trade_slot: read_u64(data, pool_offsets::LATEST_TRADE_SLOT)?,
        base_amount_in_volume: read_u64(data, pool_offsets::BASE_AMOUNT_IN_VOLUME)?,
        quote_amount_in_volume: read_u64(data, pool_offsets::QUOTE_AMOUNT_IN_VOLUME)?,
        multiply_fee_amount_in_volume: read_u64(data, pool_offsets::MULTIPLY_FEE_AMOUNT_IN_VOLUME)?,
        titan_flow_fee_ppm: read_titan_flow_fees(data)?,
    })
}

fn token_amount(account: &Account) -> Result<u64, TradingVenueError> {
    account
        .data
        .get(SPL_TOKEN_AMOUNT_OFFSET..SPL_TOKEN_AMOUNT_OFFSET + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(TradingVenueError::DeserializationFailed("token account too short".into()))
}

fn ban_entry_pda(wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[BAN_SEED, wallet.as_ref()], &KIPSELI_PROGRAM_ID).0
}

#[derive(Clone)]
pub struct KipseliVenue {
    pub pool_id: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub pool_base_ata: Pubkey,
    pub pool_quote_ata: Pubkey,
    state: PoolState,
    pool_base_reserve: u64,
    pool_quote_reserve: u64,
    unix_timestamp: i64,
    slot: u64,
    token_info: Vec<TokenInfo>,
    required_state_pubkeys: HashSet<Pubkey>,
    initialized: bool,
}

impl FromAccount for KipseliVenue {
    fn from_account(pubkey: &Pubkey, account: &Account) -> Result<Self, TradingVenueError> {
        check_pool_account(pubkey, account)?;
        let base_mint = read_pubkey(&account.data, pool_offsets::BASE_TOKEN)?;
        let quote_mint = read_pubkey(&account.data, pool_offsets::QUOTE_TOKEN)?;
        let pool_base_ata = get_associated_token_address(pubkey, &base_mint);
        let pool_quote_ata = get_associated_token_address(pubkey, &quote_mint);

        Ok(Self {
            pool_id: *pubkey,
            base_mint,
            quote_mint,
            pool_base_ata,
            pool_quote_ata,
            state: parse_pool_state(&account.data)?,
            pool_base_reserve: 0,
            pool_quote_reserve: 0,
            unix_timestamp: 0,
            slot: 0,
            token_info: Vec::new(),
            required_state_pubkeys: HashSet::from_iter([
                *pubkey,
                base_mint,
                quote_mint,
                pool_base_ata,
                pool_quote_ata,
                clock::ID,
            ]),
            initialized: false,
        })
    }
}

#[async_trait]
impl TradingVenue for KipseliVenue {
    fn initialized(&self) -> bool {
        self.initialized
    }

    fn program_id(&self) -> Pubkey {
        KIPSELI_PROGRAM_ID
    }

    fn program_dependencies(&self) -> Vec<Pubkey> {
        vec![KIPSELI_PROGRAM_ID]
    }

    fn market_id(&self) -> Pubkey {
        self.pool_id
    }

    fn tradable_mints(&self) -> Result<Vec<Pubkey>, TradingVenueError> {
        Ok(vec![self.base_mint, self.quote_mint])
    }

    fn get_token_info(&self) -> &[TokenInfo] {
        &self.token_info
    }

    fn protocol(&self) -> PoolProtocol {
        PoolProtocol::KipseliPropAmm
    }

    fn get_required_pubkeys_for_update(&self) -> Result<Vec<Pubkey>, TradingVenueError> {
        Ok(self.required_state_pubkeys.iter().cloned().collect())
    }

    async fn update_state(&mut self, cache: &dyn AccountsCache) -> Result<(), TradingVenueError> {
        let accounts = cache
            .get_accounts(&[
                self.pool_id,
                self.base_mint,
                self.quote_mint,
                self.pool_base_ata,
                self.pool_quote_ata,
                clock::ID,
            ])
            .await?;

        let [pool, base_mint, quote_mint, pool_base_ata, pool_quote_ata, clock]: [Option<Account>; 6] =
            accounts
                .try_into()
                .map_err(|_| TradingVenueError::FailedToFetchMultipleAccountData)?;

        let pool = pool.ok_or(TradingVenueError::NoAccountFound(self.pool_id.into()))?;
        let base_mint = base_mint.ok_or(TradingVenueError::NoAccountFound(self.base_mint.into()))?;
        let quote_mint =
            quote_mint.ok_or(TradingVenueError::NoAccountFound(self.quote_mint.into()))?;
        let pool_base_ata =
            pool_base_ata.ok_or(TradingVenueError::NoAccountFound(self.pool_base_ata.into()))?;
        let pool_quote_ata =
            pool_quote_ata.ok_or(TradingVenueError::NoAccountFound(self.pool_quote_ata.into()))?;
        let clock = clock.ok_or(TradingVenueError::NoAccountFound(clock::ID.into()))?;

        check_pool_account(&self.pool_id, &pool)?;
        let clock: Clock = bincode::deserialize(&clock.data)
            .map_err(|_| TradingVenueError::DeserializationFailed(clock::ID.into()))?;

        self.state = parse_pool_state(&pool.data)?;
        self.pool_base_reserve = token_amount(&pool_base_ata)?;
        self.pool_quote_reserve = token_amount(&pool_quote_ata)?;
        self.unix_timestamp = clock.unix_timestamp;
        self.slot = clock.slot;
        self.token_info = vec![
            TokenInfo::new(&self.base_mint, &base_mint, clock.epoch)?,
            TokenInfo::new(&self.quote_mint, &quote_mint, clock.epoch)?,
        ];
        self.initialized = true;

        Ok(())
    }

    fn quote(&self, request: QuoteRequest) -> Result<QuoteResult, TradingVenueError> {
        if !self.initialized {
            return Err(TradingVenueError::NotInitialized("call update_state first".into()));
        }
        if request.swap_type != SwapType::ExactIn {
            return Err(TradingVenueError::ExactOutNotSupported);
        }
        let is_base_to_quote =
            if request.input_mint == self.base_mint && request.output_mint == self.quote_mint {
                true
            } else if request.input_mint == self.quote_mint && request.output_mint == self.base_mint {
                false
            } else {
                return Err(TradingVenueError::InvalidMint(request.input_mint.into()));
            };

        let s = &self.state;
        if s.is_active == 0 {
            return Err(TradingVenueError::InactivePoolError(self.pool_id, self.protocol()));
        }
        if self.unix_timestamp.saturating_mul(1_000) >= s.price_expired_at {
            return Err(TradingVenueError::AmmMethodError("price expired".into()));
        }
        if s.raw_price == 0 {
            return Err(TradingVenueError::AmmMethodError("price is zero".into()));
        }
        let slot_gap = self.slot.saturating_sub(s.latest_block_slot);
        if s.max_slot_gap != 0 && slot_gap > s.max_slot_gap as u64 {
            return Err(TradingVenueError::AmmMethodError("price is stale".into()));
        }

        let amount = request.amount;
        let not_quotable = QuoteResult {
            input_mint: request.input_mint,
            output_mint: request.output_mint,
            amount,
            expected_output: 0,
            not_enough_liquidity: true,
            price: 0.0,
        };

        if is_base_to_quote {
            let hard_zone_threshold = s
                .curve
                .target_balance
                .checked_mul(HARD_ZONE_TARGET_MULTIPLIER)
                .ok_or_else(overflow)?;
            if self.pool_base_reserve.saturating_add(amount) >= hard_zone_threshold {
                return Ok(not_quotable);
            }
        }

        let slot_gap_fee =
            math::slot_gap_fee(slot_gap, s.slot_gap_fee_in_ppm, s.slot_gap_fee_growth_base);
        let same_slot = self.slot == s.latest_trade_slot;
        let (accumulated_volume, tiers) = if is_base_to_quote {
            (if same_slot { s.base_amount_in_volume } else { 0 }, &s.base_fee_tiers)
        } else {
            (if same_slot { s.quote_amount_in_volume } else { 0 }, &s.quote_fee_tiers)
        };

        let volume_fee = math::more_fee_from_amount_in(
            accumulated_volume,
            amount,
            s.fee_tiers_count,
            tiers,
            s.multiply_fee_amount_in_volume,
        );
        let titan_flow_fee = s
            .titan_flow_fee_ppm
            .get(request.toxicity_score as usize)
            .copied()
            .unwrap_or(0);
        let fee = s
            .fee_in_ppm
            .checked_add(volume_fee)
            .and_then(|f| f.checked_add(slot_gap_fee))
            .and_then(|f| f.checked_add(titan_flow_fee))
            .ok_or_else(overflow)?;
        if fee > MAX_PPM {
            return Ok(not_quotable);
        }

        let marginal_fee_ppm = s.fee_in_ppm as f64
            + slot_gap_fee as f64
            + titan_flow_fee as f64
            + math::more_fee_rate(
                accumulated_volume,
                amount,
                s.fee_tiers_count,
                tiers,
                s.multiply_fee_amount_in_volume,
            );
        let (base_decimals, quote_decimals) = self.decimals_u8()?;
        let rate = math::raw_rate(is_base_to_quote, base_decimals, quote_decimals, s.raw_price);

        if amount == 0 {
            return Ok(QuoteResult {
                input_mint: request.input_mint,
                output_mint: request.output_mint,
                amount: 0,
                expected_output: 0,
                not_enough_liquidity: false,
                price: math::marginal_price(
                    &s.curve,
                    self.pool_base_reserve,
                    is_base_to_quote,
                    rate,
                    marginal_fee_ppm,
                    0,
                    0,
                    0,
                ),
            });
        }

        let (out_before_curve, _) = if is_base_to_quote {
            math::calculate_quote_out(amount, base_decimals, quote_decimals, s.raw_price, fee)
                .ok_or_else(overflow)?
        } else {
            math::calculate_base_out(amount, quote_decimals, base_decimals, s.raw_price, fee)
                .ok_or_else(insufficient_liquidity)?
        };

        let (expected_output, integral) = math::adjust_to_balance(
            &s.curve,
            self.pool_base_reserve,
            is_base_to_quote,
            amount,
            out_before_curve,
        )?;

        let output_reserve = if is_base_to_quote {
            self.pool_quote_reserve
        } else {
            self.pool_base_reserve
        };
        if expected_output > output_reserve {
            return Err(insufficient_liquidity());
        }

        Ok(QuoteResult {
            input_mint: request.input_mint,
            output_mint: request.output_mint,
            amount,
            expected_output,
            not_enough_liquidity: false,
            price: math::marginal_price(
                &s.curve,
                self.pool_base_reserve,
                is_base_to_quote,
                rate,
                marginal_fee_ppm,
                amount,
                out_before_curve,
                integral,
            ),
        })
    }

    fn generate_swap_instruction(
        &self,
        request: QuoteRequest,
        user: Pubkey,
    ) -> Result<Instruction, TradingVenueError> {
        if request.swap_type != SwapType::ExactIn {
            return Err(TradingVenueError::ExactOutNotSupported);
        }
        let is_base_to_quote = if request.input_mint == self.base_mint {
            true
        } else if request.input_mint == self.quote_mint {
            false
        } else {
            return Err(TradingVenueError::InvalidMint(request.input_mint.into()));
        };

        let user_base_ata = get_associated_token_address(&user, &self.base_mint);
        let user_quote_ata = get_associated_token_address(&user, &self.quote_mint);
        let user_ban_entry = ban_entry_pda(&user);

        let mut data = Vec::with_capacity(26);
        data.extend_from_slice(&SWAP_V3_DISCRIMINATOR);
        data.extend_from_slice(&request.amount.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        data.push(is_base_to_quote as u8);
        data.push(request.toxicity_score);

        Ok(Instruction {
            program_id: KIPSELI_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new_readonly(user, true),
                AccountMeta::new(self.pool_id, false),
                AccountMeta::new(self.pool_base_ata, false),
                AccountMeta::new(self.pool_quote_ata, false),
                AccountMeta::new_readonly(self.base_mint, false),
                AccountMeta::new_readonly(self.quote_mint, false),
                AccountMeta::new(user_base_ata, false),
                AccountMeta::new(user_quote_ata, false),
                AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
                AccountMeta::new_readonly(user, false),
                AccountMeta::new_readonly(user_ban_entry, false),
                AccountMeta::new_readonly(user_ban_entry, false),
                AccountMeta::new_readonly(JITO_VOTE_ACCOUNT, false),
                AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR_ID, false),
            ],
            data,
        })
    }
}

impl KipseliVenue {
    fn decimals_u8(&self) -> Result<(u8, u8), TradingVenueError> {
        let missing = || TradingVenueError::MissingState("token_info".into());
        let base = self.token_info.first().ok_or_else(missing)?;
        let quote = self.token_info.get(1).ok_or_else(missing)?;
        Ok((base.decimals as u8, quote.decimals as u8))
    }
}

#[async_trait]
impl AddressLookupTableTrait for KipseliVenue {
    async fn get_lookup_table_keys(
        &self,
        _accounts_cache: Option<&dyn AccountsCache>,
    ) -> Result<Vec<Pubkey>, TradingVenueError> {
        Ok(vec![
            self.base_mint,
            self.quote_mint,
            TOKEN_PROGRAM_ID,
            SYSTEM_PROGRAM_ID,
            self.pool_id,
            self.pool_base_ata,
            self.pool_quote_ata,
            JITO_VOTE_ACCOUNT,
            INSTRUCTIONS_SYSVAR_ID,
            KIPSELI_PROGRAM_ID,
        ])
    }
}
