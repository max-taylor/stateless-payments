use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Serialize};

use super::common::BlsPublicKey;

// Unfortunately PublicKey does not implement the Hash trait
// And in order to use it as a key in a HashMap we need to implement the Hash trait
#[derive(Clone, Debug, Copy, Serialize)]
pub struct BlsPublicKeyWrapper(BlsPublicKey);

// The Deserializer for BlsPublicKey is pretty stupid and expects a &str instead of String, so have
// to do ALL of the below to get around that uselessness
impl<'de> Deserialize<'de> for BlsPublicKeyWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let formatted_string = format!("\"{}\"", s);

        let key: BlsPublicKey =
            serde_json::from_str(&formatted_string).map_err(serde::de::Error::custom)?;
        Ok(BlsPublicKeyWrapper(key))
    }
}

impl PartialEq for BlsPublicKeyWrapper {
    fn eq(&self, other: &Self) -> bool {
        // Implement equality as needed for PublicKey
        self.0.to_string() == other.0.to_string()
    }
}

impl Eq for BlsPublicKeyWrapper {}

impl Hash for BlsPublicKeyWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_string().hash(state);
    }
}

impl Into<BlsPublicKey> for BlsPublicKeyWrapper {
    fn into(self) -> BlsPublicKey {
        self.0
    }
}

impl From<BlsPublicKey> for BlsPublicKeyWrapper {
    fn from(public_key: BlsPublicKey) -> Self {
        BlsPublicKeyWrapper(public_key)
    }
}

impl From<&BlsPublicKey> for BlsPublicKeyWrapper {
    fn from(public_key: &BlsPublicKey) -> Self {
        BlsPublicKeyWrapper(public_key.clone())
    }
}

pub type AccountTotals = HashMap<BlsPublicKeyWrapper, u64>;
