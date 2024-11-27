use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use super::common::BlsPublicKey;

// Unfortunately PublicKey does not implement Hash, so we need to wrap it
#[derive(Clone)]
pub struct BlsPublicKeyWrapper(BlsPublicKey);

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

pub type AccountTotals = HashMap<BlsPublicKeyWrapper, u64>;
