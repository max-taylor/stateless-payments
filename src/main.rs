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

    let (txid, _) = bob.construct_transaction(alice.public_key, 100, salt);
    aggregator.add_transaction(txid);
    let (txid, _) = bob.construct_transaction(mary.public_key, 100, salt);
    aggregator.add_transaction(txid);

    let merkle_root = aggregator.generate_merkle_root()?;
    // TODO: Provide the merkle root, merkle proof to bob to verify and sign the merkle root

    println!("Hello, world!");

    Ok(())
}
