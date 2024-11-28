use std::collections::HashMap;

use bitcoincore_rpc::bitcoin::key::rand;
use blsful::{AggregateSignature, Bls12381G1Impl, BlsResult, PublicKey, SecretKey, Signature};
use rs_merkle::MerkleProof;

use crate::aggregator::Sha256Algorithm;

use super::{public_key::BlsPublicKeyWrapper, transaction::SimpleTransaction};

pub type U8_32 = [u8; 32];

pub fn generate_salt() -> U8_32 {
    rand::random()
}

type BlsType = Bls12381G1Impl;

pub type BlsPublicKey = PublicKey<BlsType>;
pub type BlsSignature = Signature<BlsType>;
pub type BlsSecretKey = SecretKey<BlsType>;
pub type BlsAggregateSignature = AggregateSignature<BlsType>;

// Need to compare TransactionProofs with TransferBlocks to find which roots have been included
#[derive(Clone, Debug, PartialEq)]
pub struct TransferBlock {
    pub aggregated_signature: BlsAggregateSignature,
    pub merkle_root: U8_32,
    pub public_keys: Vec<BlsPublicKey>,
}

impl TransferBlock {
    pub fn verify(&self) -> BlsResult<()> {
        let verify_message = self
            .public_keys
            .iter()
            .map(|pk| (pk.clone(), self.merkle_root))
            .collect::<Vec<(BlsPublicKey, U8_32)>>();

        self.aggregated_signature.verify(&verify_message)
    }
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

pub type BalanceProof =
    HashMap<(U8_32, BlsPublicKeyWrapper), (SimpleTransaction, TransactionProof)>;
