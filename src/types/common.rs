use blsful::BlsResult;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::errors::CrateResult;

use super::public_key::BlsPublicKeyWrapper;
use super::signatures::{
    BlsAggregateSignature, BlsAggregateSignatureWrapper, BlsPublicKey, BlsSignature,
    BlsSignatureWrapper,
};

pub type U8_32 = [u8; 32];

pub fn generate_salt() -> U8_32 {
    StdRng::from_entropy().gen::<U8_32>()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TransferBlockSignature {
    Aggregated(BlsAggregateSignatureWrapper, Vec<BlsPublicKeyWrapper>),
    Individual(BlsSignatureWrapper, BlsPublicKeyWrapper),
}

impl TransferBlockSignature {
    pub fn new(values: Vec<(BlsPublicKey, BlsSignature)>) -> CrateResult<Self> {
        if values.len() == 1 {
            let public_key = values[0].0.clone();
            let signature = values[0].1.clone();
            Ok(TransferBlockSignature::Individual(
                signature.into(),
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
                aggregate_signature.into(),
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

                let aggregate_signature: BlsAggregateSignature = (*sig).into();
                aggregate_signature.verify(&verify_message)
            }
            TransferBlockSignature::Individual(sig, public_key) => {
                let signature: BlsSignature = (*sig).into();
                signature.verify(&(*public_key).into(), self.merkle_root)
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
