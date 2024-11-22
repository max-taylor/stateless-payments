use sha2::{Digest, Sha256};

use super::transaction::TxHash;

pub fn hash_tx_hash_with_salt(txhash: TxHash, salt: &[u8]) -> TxHash {
    let mut hasher = Sha256::new();
    hasher.update(&txhash);
    hasher.update(salt);
    let result = hasher.finalize();
    let mut tx_hash = [0u8; 32];
    tx_hash.copy_from_slice(&result);
    tx_hash
}
