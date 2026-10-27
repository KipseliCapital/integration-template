
mod common;

use common::{RouteConfig, run_swap_route};
use solana_pubkey::Pubkey;
use titan_integration_template::your_venue::{
    KipseliVenue,
    consts::{KIPSELI_PROGRAM_ID, TEST_POOL_ADDRESS},
};

fn pool() -> Pubkey {
    TEST_POOL_ADDRESS
}

fn venue_programs() -> Vec<Pubkey> {
    vec![KIPSELI_PROGRAM_ID]
}

#[tokio::test]
async fn swap_route_both_directions() {
    run_swap_route::<KipseliVenue>(RouteConfig {
        pool: pool(),
        venue_programs: venue_programs(),
    })
    .await;
}
