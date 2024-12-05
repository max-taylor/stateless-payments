use base64::{engine::general_purpose::STANDARD, Engine as _};
use blsful::inner_types::G1Projective;
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BlsAggregateSignatureWrapper(pub BlsAggregateSignature);
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BlsSignatureWrapper(pub BlsSignature);

// TODO: This requires reparing
impl<'de> Deserialize<'de> for BlsAggregateSignatureWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize the map
        #[allow(non_snake_case)]
        #[derive(Deserialize)]
        struct MessageAugmentationWrapper {
            MessageAugmentation: String,
        }

        let wrapper = MessageAugmentationWrapper::deserialize(deserializer)?;

        // Parse the string into BlsSignature
        let key: G1Projective =
            serde_json::from_str(&format!("\"{}\"", wrapper.MessageAugmentation))
                .map_err(serde::de::Error::custom)?;

        Ok(BlsAggregateSignatureWrapper(
            BlsAggregateSignature::MessageAugmentation(key),
        ))
    }
}

impl Into<BlsAggregateSignature> for BlsAggregateSignatureWrapper {
    fn into(self) -> BlsAggregateSignature {
        self.0
    }
}

impl From<BlsAggregateSignature> for BlsAggregateSignatureWrapper {
    fn from(signature: BlsAggregateSignature) -> Self {
        BlsAggregateSignatureWrapper(signature)
    }
}

impl Into<BlsSignature> for BlsSignatureWrapper {
    fn into(self) -> BlsSignature {
        self.0
    }
}

impl From<BlsSignature> for BlsSignatureWrapper {
    fn from(signature: BlsSignature) -> Self {
        BlsSignatureWrapper(signature)
    }
}

impl<'de> Deserialize<'de> for BlsSignatureWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize the map
        #[allow(non_snake_case)]
        #[derive(Deserialize)]
        struct MessageAugmentationWrapper {
            MessageAugmentation: String,
        }

        let wrapper = MessageAugmentationWrapper::deserialize(deserializer)?;

        // Parse the string into BlsSignature
        let key: G1Projective =
            serde_json::from_str(&format!("\"{}\"", wrapper.MessageAugmentation))
                .map_err(serde::de::Error::custom)?;

        Ok(BlsSignatureWrapper(BlsSignature::MessageAugmentation(key)))
    }
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
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct BalanceProofKey {
    pub root: U8_32,
    pub public_key: BlsPublicKeyWrapper,
}
// Implement Serialize and Deserialize using a custom string representation
impl Serialize for BalanceProofKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize key as a string, e.g., base64(root) + ":" + public_key JSON
        let root_str = STANDARD.encode(&self.root);
        let public_key_str =
            serde_json::to_string(&self.public_key).map_err(serde::ser::Error::custom)?;
        let combined = format!("{}:{}", root_str, public_key_str);
        serializer.serialize_str(&combined)
    }
}

impl<'de> Deserialize<'de> for BalanceProofKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let mut parts = s.splitn(2, ':');
        let root_str = parts
            .next()
            .ok_or_else(|| serde::de::Error::custom("Missing root part in BalanceProofKey"))?;
        let public_key_str = parts.next().ok_or_else(|| {
            serde::de::Error::custom("Missing public_key part in BalanceProofKey")
        })?;

        let root = STANDARD
            .decode(root_str)
            .map_err(serde::de::Error::custom)?
            .try_into()
            .map_err(|_| serde::de::Error::custom("Invalid U8_32 length"))?;
        let public_key: BlsPublicKeyWrapper =
            serde_json::from_str(public_key_str).map_err(serde::de::Error::custom)?;

        Ok(Self { root, public_key })
    }
}
pub type BalanceProof = HashMap<BalanceProofKey, TransactionProof>;
