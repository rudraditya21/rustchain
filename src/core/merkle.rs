#![allow(dead_code)]

use crate::core::hash::{sha256, sha256_pair, Hash32};
use crate::core::transaction::Transaction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleTree {
    levels: Vec<Vec<Hash32>>,
    root: Hash32,
}

impl MerkleTree {
    pub fn from_transactions(transactions: &[Transaction]) -> Self {
        let leaves = transactions.iter().map(Transaction::tx_hash).collect();
        Self::from_leaves(leaves)
    }

    pub fn from_leaves(leaves: Vec<Hash32>) -> Self {
        if leaves.is_empty() {
            let empty_root = sha256(&[]);
            return Self {
                levels: vec![vec![empty_root]],
                root: empty_root,
            };
        }

        let mut levels = vec![leaves];

        while levels.last().map_or(0, Vec::len) > 1 {
            let current = match levels.last() {
                Some(level) => level.clone(),
                None => vec![Hash32::ZERO],
            };
            let mut next = Vec::with_capacity(current.len().div_ceil(2));

            for chunk in current.chunks(2) {
                let left = chunk[0];
                let right = if chunk.len() == 2 { chunk[1] } else { chunk[0] };
                next.push(sha256_pair(&left, &right));
            }

            levels.push(next);
        }

        let root = match levels.last().and_then(|level| level.first()) {
            Some(hash) => *hash,
            None => sha256(&[]),
        };

        Self { levels, root }
    }

    pub fn levels(&self) -> &[Vec<Hash32>] {
        &self.levels
    }

    pub fn root(&self) -> Hash32 {
        self.root
    }
}

#[cfg(test)]
mod tests {
    use crate::core::hash::{sha256, sha256_pair, Hash32};
    use crate::core::merkle::MerkleTree;
    use crate::core::transaction::Transaction;

    fn tx(nonce: u64) -> Transaction {
        Transaction {
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 50 + nonce,
            fee: 1,
            nonce,
            signature: vec![9, 9, 9],
        }
    }

    #[test]
    fn merkle_root_is_deterministic_for_same_input() {
        let txs = vec![tx(1), tx(2), tx(3)];
        let first = MerkleTree::from_transactions(&txs).root();
        let second = MerkleTree::from_transactions(&txs).root();
        assert_eq!(first, second);
    }

    #[test]
    fn odd_leaf_count_duplicates_last_leaf() {
        let txs = vec![tx(1), tx(2), tx(3)];
        let h0 = txs[0].tx_hash();
        let h1 = txs[1].tx_hash();
        let h2 = txs[2].tx_hash();

        let level1_left = sha256_pair(&h0, &h1);
        let level1_right = sha256_pair(&h2, &h2);
        let expected_root = sha256_pair(&level1_left, &level1_right);

        let tree = MerkleTree::from_transactions(&txs);
        assert_eq!(tree.root(), expected_root);
        assert_eq!(tree.levels()[0].len(), 3);
        assert_eq!(tree.levels()[1].len(), 2);
        assert_eq!(tree.levels()[2].len(), 1);
    }

    #[test]
    fn empty_merkle_tree_has_stable_root() {
        let tree = MerkleTree::from_leaves(Vec::<Hash32>::new());
        assert_eq!(tree.root(), sha256(&[]));
        assert_eq!(tree.levels().len(), 1);
        assert_eq!(tree.levels()[0].len(), 1);
    }
}
