use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use bls_signatures::{PublicKey, Serialize};

// Unfortunately PublicKey does not implement Hash, so we need to wrap it
#[derive(Clone)]
pub struct BlsPublicKeyWrapper(PublicKey);

impl PartialEq for BlsPublicKeyWrapper {
    fn eq(&self, other: &Self) -> bool {
        // Implement equality as needed for PublicKey
        self.0.as_bytes() == other.0.as_bytes()
    }
}

impl Eq for BlsPublicKeyWrapper {}

impl Hash for BlsPublicKeyWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_bytes().hash(state);
    }
}

impl Into<PublicKey> for BlsPublicKeyWrapper {
    fn into(self) -> PublicKey {
        self.0
    }
}

impl From<PublicKey> for BlsPublicKeyWrapper {
    fn from(public_key: PublicKey) -> Self {
        BlsPublicKeyWrapper(public_key)
    }
}

pub type AccountTotals = HashMap<BlsPublicKeyWrapper, u64>;
