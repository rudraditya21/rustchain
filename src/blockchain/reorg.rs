#![allow(dead_code)]

use std::collections::BTreeMap;

use crate::core::block::Block;
use crate::core::hash::Hash32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReorgDecision {
    KeepCanonical,
    AdoptFork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkStatus {
    RejectedAsLighter,
    AdoptedAsCanonical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkRecord {
    pub tip_hash: Hash32,
    pub height: u64,
    pub cumulative_work: u128,
    pub common_height: u64,
    pub status: ForkStatus,
}

#[derive(Debug, Default, Clone)]
pub struct ForkTracker {
    forks: BTreeMap<[u8; 32], ForkRecord>,
}

impl ForkTracker {
    pub fn new() -> Self {
        Self {
            forks: BTreeMap::new(),
        }
    }

    pub fn record(&mut self, record: ForkRecord) {
        self.forks.insert(record.tip_hash.0, record);
    }

    pub fn count(&self) -> usize {
        self.forks.len()
    }

    pub fn get(&self, tip_hash: &Hash32) -> Option<&ForkRecord> {
        self.forks.get(&tip_hash.0)
    }
}

pub fn block_work(block: &Block) -> u128 {
    if block.header.difficulty_bits >= 127 {
        u128::MAX
    } else {
        1u128 << block.header.difficulty_bits
    }
}

pub fn cumulative_work(blocks: &[Block]) -> u128 {
    cumulative_work_iter(blocks.iter())
}

pub fn cumulative_work_iter<'a, I>(blocks: I) -> u128
where
    I: IntoIterator<Item = &'a Block>,
{
    let mut work = 0u128;
    for block in blocks {
        work = work.saturating_add(block_work(block));
    }
    work
}

pub fn common_ancestor_height(canonical: &[Block], candidate: &[Block]) -> Option<u64> {
    let limit = canonical.len().min(candidate.len());
    let mut common = None;
    for index in 0..limit {
        if canonical[index].hash() == candidate[index].hash() {
            common = Some(index as u64);
        } else {
            break;
        }
    }
    common
}

pub fn common_ancestor_height_by_hashes(
    canonical_hashes: &[Hash32],
    candidate: &[Block],
) -> Option<u64> {
    let limit = canonical_hashes.len().min(candidate.len());
    let mut common = None;
    for index in 0..limit {
        if canonical_hashes[index] == candidate[index].hash() {
            common = Some(index as u64);
        } else {
            break;
        }
    }
    common
}
