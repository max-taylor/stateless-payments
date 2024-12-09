use rs_merkle::MerkleProof;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    aggregator::Sha256Algorithm,
    types::{common::U8_32, public_key::BlsPublicKeyWrapper},
};

use super::signatures::BlsPublicKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimpleTransaction {
    pub to: BlsPublicKey,
    pub from: BlsPublicKey,
    pub amount: u64,
    pub salt: U8_32,
}

impl<'de> Deserialize<'de> for SimpleTransaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SimpleTransactionWrapper {
            to: BlsPublicKeyWrapper,
            from: BlsPublicKeyWrapper,
            amount: u64,
            salt: U8_32,
        }

        let SimpleTransactionWrapper {
            to,
            from,
            amount,
            salt,
        } = SimpleTransactionWrapper::deserialize(deserializer)?;

        Ok(SimpleTransaction {
            to: to.into(),
            from: from.into(),
            amount,
            salt,
        })
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransactionBatch {
    pub from: BlsPublicKey,
    pub transactions: Vec<SimpleTransaction>,
}

impl<'de> Deserialize<'de> for TransactionBatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct TransactionBatchWrapper {
            from: BlsPublicKeyWrapper,
            transactions: Vec<SimpleTransaction>,
        }

        let TransactionBatchWrapper { from, transactions } =
            TransactionBatchWrapper::deserialize(deserializer)?;

        Ok(TransactionBatch {
            from: from.into(),
            transactions,
        })
    }
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
