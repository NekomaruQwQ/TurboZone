//! Public-contract tests for sequence-tagged cache insertion, refresh, and eviction.
//!
//! These tests observe returned values and factory calls rather than internal storage so the
//! cache can change representation without weakening its memoization or liveness contracts.

use std::cell::Cell;

use turbozone_core::util::Cache;

#[test]
fn get_or_insert_evaluates_factory_on_miss() {
    let calls = Cell::new(0);
    let mut cache = Cache::new();

    let value =
        cache.get_or_insert(1u64, "key", || {
            calls.set(calls.get() + 1);
            "inserted"
        });

    assert_eq!((*value, calls.get()), ("inserted", 1));
}

#[test]
fn get_or_insert_reuses_value_without_evaluating_factory() {
    let calls = Cell::new(0);
    let mut cache = Cache::new();
    let _ = cache.get_or_insert(1u64, "key", || "cached");

    let value =
        cache.get_or_insert(2, "key", || {
            calls.set(calls.get() + 1);
            "replacement"
        });

    assert_eq!((*value, calls.get()), ("cached", 0));
}

#[test]
fn get_or_insert_refreshes_sequence_on_hit() {
    let mut cache = Cache::new();
    let _ = cache.get_or_insert(1u64, "key", || "cached");
    let _ = cache.get_or_insert(3, "key", || panic!("a hit must not evaluate its factory"));

    cache.evict_before(3);

    let value =
        cache.get_or_insert(4, "key", || panic!("the refreshed entry must survive eviction"));
    assert_eq!(*value, "cached");
}

#[test]
fn evict_before_removes_only_entries_below_threshold() {
    let mut cache = Cache::new();
    let _ = cache.get_or_insert(1u64, "older", || "old value");
    let _ = cache.get_or_insert(2, "threshold", || "threshold value");
    let _ = cache.get_or_insert(3, "newer", || "new value");

    cache.evict_before(2);

    let older = *cache.get_or_insert(4, "older", || "recreated");
    let threshold =
        *cache.get_or_insert(4, "threshold", || panic!("threshold entry must remain cached"));
    let newer =
        *cache.get_or_insert(4, "newer", || panic!("newer entry must remain cached"));
    assert_eq!(
        (older, threshold, newer),
        ("recreated", "threshold value", "new value"));
}

#[test]
fn cache_uses_key_equality_without_normalization() {
    let mut cache = Cache::new();
    let _ = cache.get_or_insert(1u64, "App.exe", || "original case");

    let lowercase = *cache.get_or_insert(1, "app.exe", || "lowercase");
    let original =
        *cache.get_or_insert(1, "App.exe", || panic!("the original key must remain cached"));

    assert_eq!((original, lowercase), ("original case", "lowercase"));
}
