use bitcoincore_rpc::bitcoin::key::rand;
use bls_signatures::{PublicKey, Signature};
use rs_merkle::MerkleProof;

use crate::aggregator::Sha256Algorithm;

pub type U8_32 = [u8; 32];

pub fn generate_salt() -> U8_32 {
    rand::random()
}

// Need to compare TransactionProofs with TransferBlocks to find which roots have been included
#[derive(Clone)]
pub struct TransferBlock {
    pub aggregated_signature: Signature,
    pub merkle_root: U8_32,
    pub public_keys: Vec<PublicKey>,
}

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
