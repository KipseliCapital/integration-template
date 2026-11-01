//!
//! Like the example suite, the tests SKIP when `SOLANA_RPC_URL` (and, for the
//! simulations, dumped program binaries) are absent.

mod common;

use common::SuiteConfig;
use solana_pubkey::Pubkey;
use titan_integration_template::your_venue::{
    KipseliVenue,
    consts::{KIPSELI_PROGRAM_ID, TEST_POOL_ADDRESS},
};

// Installs the allocation guard that powers the construction test's
// `assert_no_alloc` checks. The Makefile runs that test under `release-debug`
// so the guard is active; speed tests run under true `--release`.
#[cfg(debug_assertions)]
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn pool() -> Pubkey {
    TEST_POOL_ADDRESS
}

fn programs() -> Vec<Pubkey> {
    vec![KIPSELI_PROGRAM_ID]
}

fn config() -> SuiteConfig {
    SuiteConfig {
        pool: pool(),
        programs: programs(),
    }
}

#[tokio::test]
async fn construction() {
    common::construction::<KipseliVenue>(&config()).await;
}

#[tokio::test]
async fn zero_input_spot_price() {
    common::zero_input_spot_price::<KipseliVenue>(&config()).await;
}

#[tokio::test]
async fn bound_simulation() {
    common::bound_simulation::<KipseliVenue>(&config()).await;
}

#[tokio::test]
async fn random_samples() {
    common::random_samples::<KipseliVenue>(&config()).await;
}

#[tokio::test]
async fn monotone() {
    common::monotone::<KipseliVenue>(&config()).await;
}

#[tokio::test]
async fn quoting_speed() {
    common::quoting_speed::<KipseliVenue>(&config()).await;
}

#[tokio::test]
async fn price_monotone() {
    common::price_monotone::<KipseliVenue>(&config()).await;
}

#[tokio::test]
async fn mean_value_theorem() {
    common::mean_value_theorem::<KipseliVenue>(&config()).await;
}
