use bitcoincore_rpc::jsonrpc::serde_json;
use bls_signatures::{PublicKey, Serialize};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use stateless_bitcoin_l2::types::U8_32;

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct SimpleTransaction {
    #[serde(
        serialize_with = "serialize_public_key",
        deserialize_with = "deserialize_public_key"
    )]
    pub to: PublicKey,
    #[serde(
        serialize_with = "serialize_public_key",
        deserialize_with = "deserialize_public_key"
    )]
    pub from: PublicKey,
    pub amount: u64,
}

pub fn serialize_public_key<S>(key: &PublicKey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_bytes(&key.as_bytes())
}

pub fn deserialize_public_key<'de, D>(deserializer: D) -> Result<PublicKey, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bytes = Vec::deserialize(deserializer)?;
    PublicKey::from_bytes(&bytes).map_err(serde::de::Error::custom)
}

impl Into<U8_32> for SimpleTransaction {
    fn into(self) -> U8_32 {
        let mut hasher = Sha256::new();
        hasher.update(&serde_json::to_vec(&self).unwrap());

        hasher.finalize().into()
    }
}
