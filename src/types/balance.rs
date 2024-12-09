use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use super::{common::U8_32, public_key::BlsPublicKeyWrapper, transaction::TransactionProof};

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
            serde_json::from_str(&public_key_str).map_err(serde::de::Error::custom)?;

        Ok(Self { root, public_key })
    }
}

pub type BalanceProof = HashMap<BalanceProofKey, TransactionProof>;
