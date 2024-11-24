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

    let (to_alice_txid, to_alice_tx) = bob.construct_transaction(alice.public_key, 100, salt);
    let (to_mary_txid, to_mary_tx) = bob.construct_transaction(mary.public_key, 100, salt);

    aggregator.add_transaction(to_alice_tx.clone());
    aggregator.add_transaction(to_mary_tx.clone());

    let merkle_root = aggregator.root()?;
    let merkle_total_leaves = aggregator.merkle_tree.leaves_len();

    {
        let to_alice_proof = aggregator.get_merkle_proof_for_txid(to_alice_txid)?;
        let to_alice_txid_index = aggregator.get_index_for_txid(to_alice_txid)?;

        let sigature = bob.validate_and_sign_transaction(
            merkle_root,
            to_alice_proof,
            to_alice_txid,
            to_alice_txid_index,
            merkle_total_leaves,
        )?;

        aggregator.add_signature(to_alice_tx, sigature.clone());
    }

    {
        let to_bob_proof = aggregator.get_merkle_proof_for_txid(to_mary_txid)?;
        let to_bob_txid_index = aggregator.get_index_for_txid(to_mary_txid)?;

        let sigature = bob.validate_and_sign_transaction(
            merkle_root,
            to_bob_proof,
            to_mary_txid,
            to_bob_txid_index,
            merkle_total_leaves,
        )?;

        aggregator.add_signature(to_mary_tx, sigature.clone());
    }

    let aggregated_signature = aggregator.produce_transfer_block()?;

    Ok(())
}
