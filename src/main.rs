#![allow(dead_code)]

use aggregator::Aggregator;
use client::client::Client;
use errors::StatelessBitcoinResult;
use rollup::rollup_state::MockRollupState;

mod aggregator;
mod client;
mod errors;
mod rollup;
mod types;

fn main() -> StatelessBitcoinResult<()> {
    let mut aggregator = Aggregator::new();
    let mut rollup_state = MockRollupState::new();

    let mut bob = Client::new();
    rollup_state.add_deposit(bob.public_key, 1000);

    let alice = Client::new();
    let mary = Client::new();

    // let to_alice_tx = bob.create_transaction(alice.public_key, 100)?;
    // // let to_mary_tx = bob.create_transaction(mary.public_key, 100)?;
    //
    // aggregator.add_transaction(&to_alice_tx.tx_hash(), &bob.public_key)?;
    // aggregator.add_transaction(&to_mary_tx.tx_hash(), &bob.public_key)?;

    // Create transactions to alice, mary - May need to be separate blocks
    // Get signatures
    // Add signatures to aggregator
    // Produce block
    // Update alice and mary balance
    // log balance

    //
    // let (_, to_alice_tx) = bob.construct_transaction(alice.public_key, 100);
    // let (_, to_mary_tx) = bob.construct_transaction(mary.public_key, 100);
    //
    // aggregator.add_transaction(&to_alice_tx);
    // aggregator.add_transaction(&to_mary_tx);
    //
    // {
    //     let to_alice_proof = aggregator.get_merkle_proof_for_transaction(&to_alice_tx)?;
    //
    //     let sigature = bob.validate_and_sign_transaction(to_alice_proof)?;
    //
    //     aggregator.add_signature(to_alice_tx, sigature.clone());
    // }
    //
    // {
    //     let to_bob_proof = aggregator.get_merkle_proof_for_transaction(&to_mary_tx)?;
    //
    //     let sigature = bob.validate_and_sign_transaction(to_bob_proof)?;
    //
    //     aggregator.add_signature(to_mary_tx, sigature.clone());
    // }
    //
    // let aggregated_signature = aggregator.produce_transfer_block()?;

    Ok(())
}
