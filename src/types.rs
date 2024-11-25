use bitcoincore_rpc::bitcoin::key::rand;
use bls_signatures::{PublicKey, Signature};
use rs_merkle::MerkleProof;

use crate::{aggregator::Sha256Algorithm, utils::transaction::SimpleTransaction};

pub type U8_32 = [u8; 32];

pub fn generate_salt() -> U8_32 {
    rand::random()
}

pub struct TransferBlock {
    pub aggregated_signature: Signature,
    pub merkle_root: U8_32,
    pub public_keys: Vec<PublicKey>,
}

#[derive(Clone)]
pub struct TransactionWithProof {
    pub proof_hashes: Vec<U8_32>,
    pub root: U8_32,
    pub transaction: SimpleTransaction,
    pub index: usize,
    pub total_leaves: usize,
}

impl TransactionWithProof {
    pub fn verify(&self) -> bool {
        let tx_hash = self.transaction.tx_hash();

        let merkle_proof: MerkleProof<Sha256Algorithm> =
            MerkleProof::new(self.proof_hashes.clone());

        merkle_proof.verify(self.root, &[self.index], &[tx_hash], self.total_leaves)
    }
}
