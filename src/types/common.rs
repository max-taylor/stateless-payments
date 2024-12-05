use blsful::{AggregateSignature, Bls12381G1Impl, BlsResult, PublicKey, SecretKey, Signature};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rs_merkle::MerkleProof;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{aggregator::Sha256Algorithm, errors::CrateResult};

use super::{public_key::BlsPublicKeyWrapper, transaction::TransactionBatch};

pub type U8_32 = [u8; 32];

pub fn generate_salt() -> U8_32 {
    StdRng::from_entropy().gen::<U8_32>()
}

type BlsType = Bls12381G1Impl;

pub type BlsPublicKey = PublicKey<BlsType>;
pub type BlsSignature = Signature<BlsType>;
pub type BlsSecretKey = SecretKey<BlsType>;
pub type BlsAggregateSignature = AggregateSignature<BlsType>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TransferBlockSignature {
    Aggregated(BlsAggregateSignature, Vec<BlsPublicKeyWrapper>),
    Individual(BlsSignature, BlsPublicKeyWrapper),
}

impl TransferBlockSignature {
    pub fn new(values: Vec<(BlsPublicKey, BlsSignature)>) -> CrateResult<Self> {
        if values.len() == 1 {
            let public_key = values[0].0.clone();
            let signature = values[0].1.clone();
            Ok(TransferBlockSignature::Individual(
                signature,
                public_key.into(),
            ))
        } else {
            let signatures = values
                .iter()
                .map(|(_, sig)| sig.clone())
                .collect::<Vec<BlsSignature>>();
            let aggregate_signature = BlsAggregateSignature::from_signatures(signatures)?;
            let public_keys: Vec<BlsPublicKeyWrapper> =
                values.iter().map(|(pk, _)| pk.clone().into()).collect();

            Ok(TransferBlockSignature::Aggregated(
                aggregate_signature,
                public_keys,
            ))
        }
    }
}

// Need to compare TransactionProofs with TransferBlocks to find which roots have been included
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransferBlock {
    pub signature: TransferBlockSignature,
    pub merkle_root: U8_32,
}

impl TransferBlock {
    pub fn verify(&self) -> BlsResult<()> {
        match &self.signature {
            TransferBlockSignature::Aggregated(sig, public_keys) => {
                let verify_message = public_keys
                    .iter()
                    .map(|pk| (pk.clone().into(), self.merkle_root))
                    .collect::<Vec<(BlsPublicKey, U8_32)>>();

                sig.verify(&verify_message)
            }
            TransferBlockSignature::Individual(sig, public_key) => {
                sig.verify(&(*public_key).into(), self.merkle_root)
            }
        }
    }

    pub fn contains_pubkey(&self, public_key: &BlsPublicKey) -> bool {
        match &self.signature {
            TransferBlockSignature::Aggregated(_, public_keys) => {
                public_keys.contains(&(*public_key).into())
            }
            TransferBlockSignature::Individual(_, pk) => *pk == (*public_key).into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

pub type BalanceProof = HashMap<(U8_32, BlsPublicKeyWrapper), TransactionProof>;
