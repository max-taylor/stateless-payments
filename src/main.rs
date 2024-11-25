use aggregator::Aggregator;
use client::Client;
use errors::StatelessBitcoinResult;
use types::generate_salt;

mod aggregator;
mod client;
mod errors;
mod types;
mod utils;

fn main() -> StatelessBitcoinResult<()> {
    let mut aggregator = Aggregator::new();

    let salt = generate_salt();

    let mut bob = Client::new();
    let alice = Client::new();
    let mary = Client::new();

    let (_, to_alice_tx) = bob.construct_transaction(alice.public_key, 100, salt);
    let (_, to_mary_tx) = bob.construct_transaction(mary.public_key, 100, salt);

    aggregator.add_transaction(&to_alice_tx);
    aggregator.add_transaction(&to_mary_tx);

    {
        let to_alice_proof = aggregator.get_merkle_proof_for_transaction(&to_alice_tx)?;

        let sigature = bob.validate_and_sign_transaction(to_alice_proof)?;

        aggregator.add_signature(to_alice_tx, sigature.clone());
    }

    {
        let to_bob_proof = aggregator.get_merkle_proof_for_transaction(&to_mary_tx)?;

        let sigature = bob.validate_and_sign_transaction(to_bob_proof)?;

        aggregator.add_signature(to_mary_tx, sigature.clone());
    }

    let aggregated_signature = aggregator.produce_transfer_block()?;

    Ok(())
}
