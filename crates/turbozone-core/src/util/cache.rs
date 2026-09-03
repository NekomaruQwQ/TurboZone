use std::collections::hash_map::{
    HashMap,
    Entry as HashMapEntry,
};

use std::hash::Hash;

/// A simple cache that stores values with a sequence number and evicts entries
/// that are older than a given sequence number.
#[derive(Debug, Clone)]
pub struct Cache<S, K, V>(HashMap<K, (S, V)>);

impl<S, K, V> Default for Cache<S, K, V> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<S, K, V> Cache<S, K, V> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S, K, V> Cache<S, K, V> where K: Clone + Eq + Hash {
    pub fn get_or_insert<F>(&mut self, seq: S, key: K, value_fn: F) -> &V
    where
        F: FnOnce() -> V, {
        match self.0.entry(key) {
            HashMapEntry::Occupied(entry) => {
                let &mut (ref mut entry_seq, ref value) = entry.into_mut();
                *entry_seq = seq;
                value
            },
            HashMapEntry::Vacant(entry) => {
                let &mut (_, ref value) = entry.insert((seq, value_fn()));
                value
            }
        }
    }
}

impl<S, K, V> Cache<S, K, V> where S: Copy + PartialOrd {
    pub fn evict_before(&mut self, threshold: S) {
        self.0.retain(|_, &mut (seq, _)| seq >= threshold);
    }
}
