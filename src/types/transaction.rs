use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::types::common::U8_32;

use super::common::BlsPublicKey;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleTransaction {
    #[serde(
        serialize_with = "serialize_public_key",
        deserialize_with = "deserialize_public_key"
    )]
    pub to: BlsPublicKey,
    #[serde(
        serialize_with = "serialize_public_key",
        deserialize_with = "deserialize_public_key"
    )]
    pub from: BlsPublicKey,
    pub amount: u64,
    pub salt: U8_32,
}

pub fn serialize_public_key<S>(key: &BlsPublicKey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_bytes(&key.to_string().as_bytes())
}

pub fn deserialize_public_key<'de, D>(deserializer: D) -> Result<BlsPublicKey, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bytes = Vec::deserialize(deserializer)?;
    BlsPublicKey::try_from(&bytes).map_err(serde::de::Error::custom)
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
