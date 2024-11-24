use sha2::{Digest, Sha256};

use crate::types::U8_32;

pub fn hash_tx_hash_with_salt(txhash: &U8_32, salt: &U8_32) -> U8_32 {
    let mut hasher = Sha256::new();
    hasher.update(&txhash);
    hasher.update(salt);

    hasher.finalize().into()
}
