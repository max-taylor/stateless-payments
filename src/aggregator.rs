use bitcoincore_rpc::bitcoin::{merkle_tree::PartialMerkleTree, TxMerkleNode, Txid};

use crate::errors::StatelessBitcoinResult;

pub struct Aggregator {
    pub txids: Vec<Txid>,
}

impl Aggregator {
    pub fn new() -> Aggregator {
        Aggregator { txids: Vec::new() }
    }

    pub fn add_transaction(&mut self, txid: Txid) {
        self.txids.push(txid);
    }

    pub fn generate_merkle_root(&self) -> StatelessBitcoinResult<TxMerkleNode> {
        let matches: Vec<bool> = vec![true; self.txids.len()];

        let tree = PartialMerkleTree::from_txids(&self.txids, &matches);

        let mut matches_out = Vec::new();
        let mut indexes_out = Vec::new();
        let merkle_root = tree.extract_matches(&mut matches_out, &mut indexes_out)?;

        Ok(merkle_root)
    }
}
