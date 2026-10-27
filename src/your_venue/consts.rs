use solana_pubkey::Pubkey;

pub const KIPSELI_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("3TK9D8aoBFYjYZtKCjciPrVrRStsnvo7KmpcJqDavpaU");

pub const TEST_POOL_ADDRESS: Pubkey =
    Pubkey::from_str_const("7yD7gfXv49SFxhx9uTc11dEd7Nj98rGWeYMdZJgheZbG");

pub const JITO_VOTE_ACCOUNT: Pubkey =
    Pubkey::from_str_const("J1to1yufRnoWn81KYg1XkTWzmKjnYSnmE2VY8DGUJ9Qv");

pub const MAX_PPM: u64 = 1_000_000;
pub const PPM_PRECISION: u64 = 10_000;
pub const PRICE_SCALE: u64 = 1_000_000_000;
pub const CURVE_SCALE: i64 = 1_000_000;

pub const HARD_ZONE_TARGET_MULTIPLIER: u64 = 3;

pub const FEE_TIER_SIZE: usize = 16;
pub const MAX_FEE_TIERS: usize = 7;

pub const DISCRIMINATOR_LEN: usize = 8;

pub const SWAP_V3_DISCRIMINATOR: [u8; 8] = [240, 224, 38, 33, 176, 31, 241, 175];

pub const CREATE_POOL_DISCRIMINATOR: [u8; 8] = [233, 146, 209, 142, 207, 104, 64, 188];

pub const POOL_DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];

pub const BAN_SEED: &[u8] = b"ban";

pub mod pool_offsets {
    use super::DISCRIMINATOR_LEN;
    pub const BASE_TOKEN: usize = DISCRIMINATOR_LEN + 64;
    pub const QUOTE_TOKEN: usize = BASE_TOKEN + 32;
    pub const RAW_PRICE: usize = QUOTE_TOKEN + 32;
    pub const PRICE_EXPIRED_AT: usize = RAW_PRICE + 8;
    pub const FEE_IN_PPM: usize = PRICE_EXPIRED_AT + 8;
    pub const IS_ACTIVE: usize = FEE_IN_PPM + 8;
    pub const FEE_TIERS_COUNT: usize = IS_ACTIVE + 2;
    pub const MAX_SLOT_GAP: usize = FEE_TIERS_COUNT + 1;
    pub const BASE_FEE_TIERS: usize = MAX_SLOT_GAP + 1 + 4;
    pub const QUOTE_FEE_TIERS: usize = BASE_FEE_TIERS + 112;
    pub const A: usize = QUOTE_FEE_TIERS + 112;
    pub const B: usize = A + 8;
    pub const C: usize = B + 8;
    pub const TARGET_BALANCE: usize = C + 8;
    pub const BAD_PENALTY_IN_PPM: usize = TARGET_BALANCE + 8;
    pub const LATEST_BLOCK_SLOT: usize = BAD_PENALTY_IN_PPM + 8;
    pub const SLOT_GAP_FEE_IN_PPM: usize = LATEST_BLOCK_SLOT + 8;
    pub const BAD_CURVE_MULTIPLIER_PPM: usize = SLOT_GAP_FEE_IN_PPM + 16;
    pub const LATEST_TRADE_SLOT: usize = BAD_CURVE_MULTIPLIER_PPM + 8;
    pub const BASE_AMOUNT_IN_VOLUME: usize = LATEST_TRADE_SLOT + 8;
    pub const QUOTE_AMOUNT_IN_VOLUME: usize = BASE_AMOUNT_IN_VOLUME + 8;
    pub const MULTIPLY_FEE_AMOUNT_IN_VOLUME: usize = QUOTE_AMOUNT_IN_VOLUME + 344;
    pub const MIN_SLIPPAGE_IN_PPM: usize = MULTIPLY_FEE_AMOUNT_IN_VOLUME + 8;
    pub const SLOT_GAP_FEE_GROWTH_BASE: usize = MIN_SLIPPAGE_IN_PPM + 8;
    pub const MIN_LEN: usize = SLOT_GAP_FEE_GROWTH_BASE + 8;
}

pub mod create_pool_accounts {
    pub const POOL: usize = 4;
    pub const BASE_TOKEN: usize = 7;
    pub const QUOTE_TOKEN: usize = 8;
}

pub const SPL_TOKEN_AMOUNT_OFFSET: usize = 64;
