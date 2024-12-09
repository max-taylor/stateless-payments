use rs_merkle::MerkleProof;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{aggregator::Sha256Algorithm, types::common::U8_32};

use super::signatures::BlsPublicKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleTransaction {
    pub to: BlsPublicKey,
    pub from: BlsPublicKey,
    pub amount: u64,
    pub salt: U8_32,
}

impl Into<U8_32> for SimpleTransaction {
    fn into(self) -> U8_32 {
        let mut hasher = Sha256::new();
        hasher.update(&serde_json::to_vec(&self).unwrap());

        hasher.finalize().into()
    }
}

impl SimpleTransaction {
    // Calculate the hash of the transaction including the salt
    pub fn tx_hash(&self) -> U8_32 {
        let txhash: U8_32 = self.clone().into();
        let mut hasher = Sha256::new();
        hasher.update(&txhash);
        hasher.update(self.salt);

        hasher.finalize().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionBatch {
    pub from: BlsPublicKey,
    pub transactions: Vec<SimpleTransaction>,
}

impl TransactionBatch {
    pub fn new(from: BlsPublicKey) -> Self {
        TransactionBatch {
            from,
            transactions: Vec::new(),
        }
    }

    pub fn tx_hash(&self) -> U8_32 {
        let mut hasher = Sha256::new();
        for tx in &self.transactions {
            hasher.update(&tx.tx_hash());
        }

        hasher.finalize().into()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionProof {
    pub proof_hashes: Vec<U8_32>,
    pub root: U8_32,
    pub batch: TransactionBatch,
    pub index: usize,
    pub total_leaves: usize,
}

impl TransactionProof {
    pub fn verify(&self) -> bool {
        let merkle_proof: MerkleProof<Sha256Algorithm> =
            MerkleProof::new(self.proof_hashes.clone());

        merkle_proof.verify(
            self.root,
            &[self.index],
            &[self.batch.tx_hash()],
            self.total_leaves,
        )
    }
}
