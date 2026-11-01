
use solana_pubkey::{Pubkey, pubkey};

use titan_integration_template::trading_venue::protocol::PoolProtocol;
use titan_integration_template::trading_venue::venue_creation::{ParsedInstruction, PoolCreation};
use titan_integration_template::your_venue::{
    consts::{CREATE_POOL_DISCRIMINATOR, KIPSELI_PROGRAM_ID, SWAP_V3_DISCRIMINATOR},
    parse_pool_creations,
};

const POOL: Pubkey = pubkey!("7yD7gfXv49SFxhx9uTc11dEd7Nj98rGWeYMdZJgheZbG");
const TOKEN_A_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
const TOKEN_B_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const POOL_BASE_ATA: Pubkey = pubkey!("HgFLBWQugxt6qbPaWiwXjrqauXdQmcpMV6H6t8ttD8s1");
const POOL_QUOTE_ATA: Pubkey = pubkey!("FzvRbXDdrsYVMa9KohXyQytWNAZAo4PsF8yujDH7KCfG");

fn create_pool_instruction(program_id: Pubkey, discriminator: [u8; 8]) -> ParsedInstruction {
    let mut data = discriminator.to_vec();
    data.extend_from_slice(&[0u8; 64]);
    ParsedInstruction {
        program_id,
        accounts: vec![
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            POOL,
            POOL_BASE_ATA,
            POOL_QUOTE_ATA,
            TOKEN_A_MINT,
            TOKEN_B_MINT,
            pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
            pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"),
            pubkey!("11111111111111111111111111111111"),
        ],
        data,
    }
}

fn unrelated_instruction() -> ParsedInstruction {
    ParsedInstruction {
        program_id: KIPSELI_PROGRAM_ID,
        accounts: vec![],
        data: vec![],
    }
}

#[test]
fn parses_your_venue_pool_creation() {
    let creations =
        parse_pool_creations(&[create_pool_instruction(KIPSELI_PROGRAM_ID, CREATE_POOL_DISCRIMINATOR)]);

    assert_eq!(
        creations,
        vec![PoolCreation {
            protocol: PoolProtocol::KipseliPropAmm,
            pool: POOL,
            mints: vec![TOKEN_A_MINT, TOKEN_B_MINT],
        }],
    );
}

#[test]
fn ignores_transactions_without_a_creation() {
    let creations = parse_pool_creations(&[
        unrelated_instruction(),
        create_pool_instruction(KIPSELI_PROGRAM_ID, SWAP_V3_DISCRIMINATOR),
        create_pool_instruction(Pubkey::new_unique(), CREATE_POOL_DISCRIMINATOR),
    ]);
    assert!(
        creations.is_empty(),
        "a transaction without a pool creation creates no pools, got {creations:?}"
    );
}
