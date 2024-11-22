use bitcoincore_rpc::{
    bitcoin::{
        hashes::{sha256d, Hash},
        Txid,
    },
    jsonrpc::serde_json,
};
use bls_signatures::{PublicKey, Serialize};
use serde::Deserialize;

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

impl Into<Txid> for SimpleTransaction {
    fn into(self) -> Txid {
        sha256d::Hash::hash(&serde_json::to_vec(&self).unwrap()).into()
    }
}
