use std::collections::HashMap;

use bitcoincore_rpc::bitcoin::key::rand::thread_rng;
use bls_signatures::{PrivateKey, PublicKey};
use rs_merkle::MerkleProof;
use sha2::Sha256;
use stateless_bitcoin_l2::types::U8_32;

use crate::{
    errors::StatelessBitcoinResult,
    utils::{hashing::hash_tx_hash_with_salt, transaction::SimpleTransaction},
};

pub struct Client {
    pub public_key: bls_signatures::PublicKey,
    pub private_key: bls_signatures::PrivateKey,
    pub transaction_history: HashMap<U8_32, SimpleTransaction>,

    pub balance: u64,
}

impl Client {
    pub fn new() -> Client {
        let private_key = PrivateKey::generate(&mut thread_rng());

        Client {
            private_key,
            public_key: private_key.public_key(),
            transaction_history: HashMap::new(),
            balance: 0,
        }
    }

    pub fn construct_transaction(
        &mut self,
        to: PublicKey,
        amount: u64,
        salt: U8_32,
    ) -> (U8_32, SimpleTransaction) {
        let transaction = SimpleTransaction {
            to,
            from: self.public_key,
            amount,
        };
        let tx_hash = hash_tx_hash_with_salt(&transaction.clone().into(), &salt);

        self.transaction_history
            .insert(tx_hash, transaction.clone());

        (tx_hash, transaction.into())
    }

    pub fn add_transaction(
        &mut self,
        merkle_root: U8_32,
        merkle_proof: U8_32,
        transaction: SimpleTransaction,
        salt: U8_32,
    ) -> StatelessBitcoinResult<()> {
        if transaction.to != self.public_key || transaction.from != self.public_key {
            return Err(anyhow::anyhow!("Invalid transaction"));
        }

        let tx_hash = hash_tx_hash_with_salt(&transaction.clone().into(), &salt);

        self.transaction_history
            .insert(tx_hash, transaction.clone());

        self.balance += transaction.amount;

        Ok(())
    }
}

/// Verifies if a transaction hash `tx_hash` is part of the Merkle tree rooted at `merkle_root`
/// by using `merkle_proof`, a list of hashes that represent the path to the root.
///
/// # Arguments
/// * `tx_hash` - The hash of the transaction we want to verify.
/// * `merkle_proof` - The proof elements (hashes) leading to the root of the Merkle tree.
/// * `merkle_root` - The root of the Merkle tree.
///
/// # Returns
/// * `bool` - Returns `true` if the proof is valid, otherwise `false`.
fn verify_merkle_proof(tx_hash: U8_32, merkle_proof: Vec<U8_32>, merkle_root: U8_32) -> bool {
    // Convert U8_32 types to byte arrays for use with rs_merkle
    let tx_hash_bytes: [u8; 32] = tx_hash.into();
    let merkle_root_bytes: [u8; 32] = merkle_root.into();
    let proof_hashes: Vec<[u8; 32]> = merkle_proof.into_iter().map(|hash| hash.into()).collect();

    // Create a MerkleProof object using rs_merkle
    let proof = MerkleProof::<Sha256>::new(proof_hashes);

    // Verify that `tx_hash` leads to `merkle_root` using the given proof
    proof
        .verify(
            &merkle_root_bytes,
            &tx_hash_bytes,
            // Index of the transaction in the Merkle tree (this needs to be known or provided)
            0, // Replace `0` with the actual index of tx_hash in the Merkle tree if known
        )
        .is_ok()
}
