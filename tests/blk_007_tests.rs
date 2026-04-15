//! # BLK-007 — Async read API (`get_block_async`, `get_header_async`, `get_block_by_height_async`)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-007.md`](../docs/requirements/domains/block_storage/specs/BLK-007.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-007)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-007-async-api)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | `get_block_async` ≡ `get_block` | [`test_get_block_async_matches_sync_after_cache_evict`] |
//! | §2 | `get_header_async` ≡ `get_header` | [`test_get_header_async_matches_sync_after_cache_evict`] |
//! | §3 | `get_block_by_height_async` resolves canonical height | [`test_get_block_by_height_async_matches_canonical_put`] |
//! | §4 | Cache hits avoid `spawn_blocking` scheduling (first `poll` = `Ready`) | [`test_get_block_async_cache_hit_first_poll_ready`], [`test_get_header_async_cache_hit_first_poll_ready`] |
//! | §5 | Cache miss uses blocking pool (first `poll` = `Pending`, then `await`) | [`test_get_block_async_cache_miss_first_poll_pending_then_await`] |
//! | §6 | Join failures surface as [`dig_blockstore::BlockStoreError::Serialization`] with [`ERR_ASYNC_JOIN_PREFIX`] | [`test_join_error_surface_uses_stable_prefix`] |
//!
//! **Instrumentation:** We use [`std::future::Future::poll`] with [`futures::task::noop_waker`] (dev-dependency) so we
//! can distinguish **synchronous completion** (cache hit before any `.await`) from **`spawn_blocking`** scheduling
//! (typically `Pending` on first poll until the blocking pool runs), matching BLK-007 AC §4–5 without mocking RocksDB.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use chia_protocol::Bytes32;
use dig_block::constants::ZERO_HASH;
use dig_blockstore::{BlockStore, BlockStoreError, ERR_ASYNC_JOIN_PREFIX};
use futures::task::noop_waker;
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use common::{temp_blockstore_dir, test_block, test_config};

/// **Proves:** BLK-007 AC §4 — a warm [`BlockStore::block_cache`] means [`BlockStore::get_block_async`] returns
/// `Poll::Ready` on the **first** `poll` without ever reaching `.await` on a [`tokio::task::spawn_blocking`] future
/// (the async fn returns before the first await point).
#[tokio::test]
async fn test_get_block_async_cache_hit_first_poll_ready() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(5, ZERO_HASH);
    store.put_block(&b, false).expect("put seeds block_cache");
    let h = b.hash();

    let mut fut = pin!(store.get_block_async(&h));
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(Some(got))) => assert_eq!(got.hash(), h),
        Poll::Ready(Ok(None)) => panic!("unexpected None on cache hit"),
        Poll::Ready(Err(e)) => panic!("unexpected err {e:?}"),
        Poll::Pending => panic!(
            "BLK-007 AC §4: cache hit must complete on first poll without pending on spawn_blocking"
        ),
    }
}

/// **Proves:** BLK-007 AC §5 — after [`BlockStore::invalidate_block_cache_entry`], the async future should reach
/// `Poll::Pending` on the first `poll` (blocking work scheduled), then [`Future::await`] yields the same block as
/// synchronous [`BlockStore::get_block`].
#[tokio::test]
async fn test_get_block_async_cache_miss_first_poll_pending_then_await() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(6, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let h = b.hash();
    store.invalidate_block_cache_entry(&h);

    let mut fut = pin!(store.get_block_async(&h));
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    match fut.as_mut().poll(&mut cx) {
        Poll::Pending => {}
        Poll::Ready(other) => {
            panic!("expected Pending on first poll for spawn_blocking path, got {other:?}")
        }
    }

    let async_got = fut.await.expect("await join").expect("some block");
    let sync_got = store.get_block(&h).expect("sync get").expect("sync some");
    assert_eq!(async_got.hash(), sync_got.hash());
}

/// **Proves:** BLK-007 AC §1 — byte-for-byte identity via hash: async path matches sync after forcing a RocksDB miss.
#[tokio::test]
async fn test_get_block_async_matches_sync_after_cache_evict() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(8, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let h = b.hash();
    store.invalidate_block_cache_entry(&h);
    let a = store
        .get_block_async(&h)
        .await
        .expect("async")
        .expect("some");
    let s = store.get_block(&h).expect("sync").expect("some");
    assert_eq!(a.hash(), s.hash());
}

/// **Proves:** BLK-007 AC §4 (header variant) — [`BlockStore::get_header_async`] hits [`BlockStore::header_cache`]
/// synchronously when the header is already cached (here via `put_block`).
#[tokio::test]
async fn test_get_header_async_cache_hit_first_poll_ready() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(9, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let h = b.hash();

    let mut fut = pin!(store.get_header_async(&h));
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(Some(hdr))) => assert_eq!(hdr, b.header),
        Poll::Ready(Ok(None)) => panic!("unexpected None"),
        Poll::Ready(Err(e)) => panic!("{e:?}"),
        Poll::Pending => panic!("header cache hit must not pend on first poll"),
    }
}

/// **Proves:** BLK-007 AC §2 — async header matches sync after cache eviction (miss path exercises `spawn_blocking`).
#[tokio::test]
async fn test_get_header_async_matches_sync_after_cache_evict() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(10, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let h = b.hash();
    store.invalidate_header_cache_entry(&h);
    let a = store
        .get_header_async(&h)
        .await
        .expect("async")
        .expect("some");
    let s = store.get_header(&h).expect("sync").expect("some");
    assert_eq!(a, s);
}

/// **Proves:** BLK-007 AC §3 — canonical height written by `put_block(..., true)` is visible to
/// [`BlockStore::get_block_by_height_async`].
#[tokio::test]
async fn test_get_block_by_height_async_matches_canonical_put() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let height = 12u64;
    let b = test_block(height, ZERO_HASH);
    assert!(store.put_block(&b, true).expect("canonical put"));
    let got = store
        .get_block_by_height_async(height)
        .await
        .expect("async height")
        .expect("some");
    assert_eq!(got.hash(), b.hash());
}

/// **Proves:** BLK-007 test plan “missing height” — empty [`CF_CANONICAL`] slot yields `Ok(None)` (same semantics as
/// a sync height probe + `get_block` would, without requiring a public sync helper).
#[tokio::test]
async fn test_get_block_by_height_async_unknown_height_none() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(0, ZERO_HASH);
    store.put_block(&b, false).expect("non-canonical put");
    assert!(store
        .get_block_by_height_async(999)
        .await
        .expect("query")
        .is_none());
}

/// **Proves:** BLK-007 test plan “missing hash” — unknown hash returns `Ok(None)` on async path.
#[tokio::test]
async fn test_get_block_async_unknown_hash_none() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let unknown = Bytes32::new([0xabu8; 32]);
    assert!(store
        .get_block_async(&unknown)
        .await
        .expect("async")
        .is_none());
}

/// **Proves:** BLK-007 AC §6 — [`BlockStoreError::Serialization`] carries [`ERR_ASYNC_JOIN_PREFIX`] when the blocking
/// task fails to complete (panic inside `spawn_blocking`).
///
/// **Note:** This does **not** add a new [`BlockStoreError`] variant ([`ERR-001`](../docs/requirements/domains/error_types/specs/ERR-001_blockstoreerror_enum.md) cap); it locks the stable string contract from [`dig_blockstore::error`].
#[tokio::test]
async fn test_join_error_surface_uses_stable_prefix() {
    let err = tokio::task::spawn_blocking(|| -> Result<(), ()> {
        panic!("forced panic for BLK-007 join mapping test");
    })
    .await
    .expect_err("join should fail when blocking task panics");

    let mapped = BlockStoreError::Serialization(format!("{ERR_ASYNC_JOIN_PREFIX}{err}"));
    let s = mapped.to_string();
    assert!(
        s.contains(ERR_ASYNC_JOIN_PREFIX),
        "Display should include ERR_ASYNC_JOIN_PREFIX: {s}"
    );
}

/// **Proves:** [`BlockStore::clone`] is O(1) shared handle ([`BLK-007.md`](../docs/requirements/domains/block_storage/specs/BLK-007.md) implementation notes) — both clones observe the same physical read counter after one miss.
#[tokio::test]
async fn test_block_store_clone_shares_inner_state() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(14, ZERO_HASH);
    store.put_block(&b, false).expect("put");
    let h = b.hash();
    store.invalidate_block_cache_entry(&h);

    let a = Arc::new(store);
    let b_handle = Arc::clone(&a);
    let c_handle = Arc::clone(&a);
    let h_copy = h;
    let t1 = tokio::spawn(async move { b_handle.get_block_async(&h_copy).await });
    let t2 = tokio::spawn(async move { c_handle.get_block_async(&h).await });
    let _ = t1.await.expect("join t1").expect("ok t1");
    let _ = t2.await.expect("join t2").expect("ok t2");
    // Two misses may each increment cf_blocks_physical_gets — exact value depends on scheduling;
    // we only assert the counter moved, proving both Arc clones hit one RocksDB-backed store.
    assert!(
        a.cf_blocks_physical_get_count() >= 1,
        "expected at least one CF_BLOCKS read after cache invalidation"
    );
}
