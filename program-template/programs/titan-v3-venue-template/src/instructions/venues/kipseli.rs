use anchor_lang::{prelude::*, solana_program::instruction::Instruction};

pub const PROGRAM_ID: Pubkey = pubkey!("3TK9D8aoBFYjYZtKCjciPrVrRStsnvo7KmpcJqDavpaU");

const SWAP_V3_DISCRIMINATOR: [u8; 8] = [240, 224, 38, 33, 176, 31, 241, 175];

pub fn swap_v3(
    is_base_to_quote: bool,
    amount_in: u64,
    account_metas: &[AccountMeta],
) -> Result<Vec<Instruction>> {
    let mut data = Vec::with_capacity(25);
    data.extend_from_slice(&SWAP_V3_DISCRIMINATOR);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.push(is_base_to_quote as u8);

    Ok(vec![Instruction {
        program_id: PROGRAM_ID,
        accounts: account_metas.to_vec(),
        data,
    }])
}
