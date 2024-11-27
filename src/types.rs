use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use bitcoincore_rpc::bitcoin::key::rand;
use bls_signatures::{PublicKey, Serialize, Signature};
use rs_merkle::MerkleProof;

use crate::aggregator::Sha256Algorithm;

pub type U8_32 = [u8; 32];

pub fn generate_salt() -> U8_32 {
    rand::random()
}

// Need to compare TransactionProofs with TransferBlocks to find which roots have been included
#[derive(Clone, Debug, PartialEq)]
pub struct TransferBlock {
    pub aggregated_signature: Signature,
    pub merkle_root: U8_32,
    pub public_keys: Vec<PublicKey>,
}

// Unfortunately PublicKey does not implement Hash, so we need to wrap it
#[derive(Clone)]
pub struct PublicKeyWrapper(PublicKey);

impl PartialEq for PublicKeyWrapper {
    fn eq(&self, other: &Self) -> bool {
        // Implement equality as needed for PublicKey
        self.0.as_bytes() == other.0.as_bytes()
    }
}

impl Eq for PublicKeyWrapper {}

impl Hash for PublicKeyWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state);
    }
}

impl Into<PublicKey> for PublicKeyWrapper {
    fn into(self) -> PublicKey {
        self.0
    }
}

impl From<PublicKey> for PublicKeyWrapper {
    fn from(public_key: PublicKey) -> Self {
        PublicKeyWrapper(public_key)
    }
}

pub type AccountTotals = HashMap<PublicKeyWrapper, u64>;

#[derive(Clone)]
pub struct TransactionProof {
    pub proof_hashes: Vec<U8_32>,
    pub root: U8_32,
    pub tx_hash: U8_32,
    pub index: usize,
    pub total_leaves: usize,
}

impl TransactionProof {
    pub fn verify(&self) -> bool {
        let merkle_proof: MerkleProof<Sha256Algorithm> =
            MerkleProof::new(self.proof_hashes.clone());

        merkle_proof.verify(self.root, &[self.index], &[self.tx_hash], self.total_leaves)
    }
}
