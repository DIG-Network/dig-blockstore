//! # BLK-008 — Write pipeline (`put_pipelined`, batched `WriteBatch`)
//!
//! **Trace (`docs/prompt/start.md`)**
//! - Spec + test plan: [`BLK-008.md`](../docs/requirements/domains/block_storage/specs/BLK-008.md)
//! - NORMATIVE: [`NORMATIVE.md` (BLK-008)](../docs/requirements/domains/block_storage/NORMATIVE.md#blk-008-write-pipeline-put_pipelined)
//! - Verification: [`VERIFICATION.md`](../docs/requirements/domains/block_storage/VERIFICATION.md)
//!
//! ## Acceptance criteria mapping
//!
//! | AC | Meaning | Test |
//! |----|---------|------|
//! | §1 | Ingress via bounded channel, not direct `put_block` in caller | [`test_put_pipelined_uses_ack_channels`] |
//! | §2 | Batch up to `write_pipeline_batch_size` | [`test_exact_batch_size_single_rocksdb_write_batch`] |
//! | §3 | Partial batch flushed after `write_pipeline_flush_ms` | [`test_partial_batch_flush_after_timeout`] |
//! | §4 | One `WriteBatch` per flush | [`test_exact_batch_size_single_rocksdb_write_batch`] (instrumented counter) |
//! | §5 | Idempotency (duplicate hash) | [`test_pipeline_duplicate_hash_second_ack_false`] |
//! | §6–§7 | Record + canonical index parity with sync path | [`test_canonical_height_visible_after_pipeline`] |
//! | §8 | Channel close drains tail before worker exit | [`test_graceful_shutdown_flushes_unacked_partial_batch`] |
//! | Test plan | Bounded `mpsc` backpressure (no loss) | [`test_bounded_channel_backpressure_without_loss`] |
//!
//! **Runtime:** [`BlockStore::put_pipelined`](dig_blockstore::BlockStore::put_pipelined) lazily spawns a worker with
//! [`tokio::spawn`]; every test uses `#[tokio::test]` so [`tokio::runtime::Handle::try_current`] succeeds.

#![forbid(unsafe_code)]

#[path = "common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::time::Duration;

use dig_block::constants::ZERO_HASH;
use dig_block::L2Block;
use dig_blockstore::{BlockStore, BlockStoreConfig, BlockStoreError};

use common::{build_chain, temp_blockstore_dir, test_block, test_config};

/// Wide batching window: `write_pipeline_batch_size = 4` with a **moderate** flush timer.
///
/// **Why not 30s:** AC §2 requires four jobs in one worker buffer before flush; that only happens when callers
/// enqueue multiple [`BlockStore::put_pipelined`] futures **without** awaiting each oneshot between sends (see
/// [`test_exact_batch_size_single_rocksdb_write_batch`]). A multi-second partial-batch timer would make those tests
/// hang and still would not create multi-block batches if each `rx.await` runs before the next `put_pipelined`.
///
/// **800ms** is long enough for CI jitter when four jobs are queued back-to-back on the async executor, yet short
/// enough that single-block tests (e.g. duplicate idempotency) still finish quickly
/// ([`BLK-008.md`](../docs/requirements/domains/block_storage/specs/BLK-008.md) test plan).
fn pipeline_wide_flush_config_fixed(path: PathBuf) -> BlockStoreConfig {
    BlockStoreConfig {
        path: path.clone(),
        write_pipeline_batch_size: 4,
        write_pipeline_flush_ms: 800,
        write_pipeline_channel_capacity: 64,
        ..test_config(path)
    }
}

/// Tight timer so a **partial** batch flushes without reaching `batch_size` (BLK-008 AC §3).
fn pipeline_fast_timeout_config(path: PathBuf) -> BlockStoreConfig {
    BlockStoreConfig {
        path: path.clone(),
        write_pipeline_batch_size: 64,
        write_pipeline_flush_ms: 80,
        write_pipeline_channel_capacity: 128,
        ..test_config(path)
    }
}

/// **Proves:** BLK-008 AC §1 — `put_pipelined` returns a [`tokio::sync::oneshot::Receiver`] that completes with the same
/// `Result<bool, _>` shape as [`BlockStore::put_block`] for a novel insert (`Ok(true)`).
#[tokio::test]
async fn test_put_pipelined_uses_ack_channels() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(test_config(path)).expect("open");
    let b = test_block(3, ZERO_HASH);
    let rx = store
        .put_pipelined(b.clone(), false)
        .await
        .expect("enqueue");
    let inserted = rx.await.expect("oneshot").expect("store result");
    assert!(inserted);
    let got = store.get_block(&b.hash()).expect("get").expect("some");
    assert_eq!(got.hash(), b.hash());
}

/// **Proves:** BLK-008 AC §2 + §4 — exactly `write_pipeline_batch_size` (4) jobs coalesce into **one**
/// [`rocksdb::WriteBatch`] commit, counted by [`BlockStore::pipeline_write_batch_count`].
#[tokio::test]
async fn test_exact_batch_size_single_rocksdb_write_batch() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(pipeline_wide_flush_config_fixed(path)).expect("open");
    let chain = build_chain(8);
    // **Critical:** `put_pipelined` only returns after the job is queued (`mpsc::send`), not after durable write.
    // We must enqueue four jobs *before* awaiting any oneshot; otherwise the worker holds ≤1 block until the flush
    // timer fires and emits one WriteBatch per block (violating AC §2 “batch up to batch_size”).
    let mut acks = Vec::new();
    for b in chain.iter().skip(1).take(4) {
        acks.push(
            store
                .put_pipelined(b.clone(), false)
                .await
                .expect("enqueue"),
        );
    }
    for rx in acks {
        let _ = rx.await.expect("ack join").expect("insert ok");
    }
    assert_eq!(
        store.pipeline_write_batch_count(),
        1,
        "four inserts with batch_size=4 and long flush window must produce exactly one WriteBatch"
    );
}

/// **Proves:** BLK-008 AC §3 — fewer than `batch_size` items are flushed once the flush timer elapses.
#[tokio::test]
async fn test_partial_batch_flush_after_timeout() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(pipeline_fast_timeout_config(path)).expect("open");
    let a = test_block(1, ZERO_HASH);
    let b = test_block(2, a.hash());
    let ra = store.put_pipelined(a.clone(), false).await.expect("a");
    let rb = store.put_pipelined(b.clone(), false).await.expect("b");
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(ra.await.expect("join a").expect("ok a"));
    assert!(rb.await.expect("join b").expect("ok b"));
    assert!(
        store.pipeline_write_batch_count() >= 1,
        "timer-driven flush should commit at least one WriteBatch"
    );
}

/// **Proves:** BLK-008 AC §5 — duplicate hash yields `Ok(false)` on the second ack without a second physical row.
#[tokio::test]
async fn test_pipeline_duplicate_hash_second_ack_false() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(pipeline_wide_flush_config_fixed(path)).expect("open");
    let b = test_block(5, ZERO_HASH);
    let r1 = store.put_pipelined(b.clone(), false).await.expect("first");
    assert!(r1.await.expect("j1").expect("insert"));
    let r2 = store.put_pipelined(b.clone(), false).await.expect("second");
    assert!(
        !r2.await.expect("j2").expect("dup"),
        "second enqueue of same hash must be idempotent Ok(false)"
    );
    assert_eq!(store.pipeline_write_batch_count(), 1);
}

/// **Proves:** BLK-008 AC §7 — canonical flag writes [`CF_CANONICAL`] rows discoverable via height probe
/// ([`BlockStore::get_block_by_height_async`]).
#[tokio::test]
async fn test_canonical_height_visible_after_pipeline() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(pipeline_wide_flush_config_fixed(path)).expect("open");
    let h = 9u64;
    let b = test_block(h, ZERO_HASH);
    let rx = store.put_pipelined(b.clone(), true).await.expect("pipe");
    assert!(rx.await.expect("ack").expect("insert"));
    let got = store
        .get_block_by_height_async(h)
        .await
        .expect("height query")
        .expect("block");
    assert_eq!(got.hash(), b.hash());
}

/// **Proves:** BLK-008 integration test plan — bulk ingest (32 blocks) then random `get_block` reads all succeed.
#[tokio::test]
async fn test_pipelined_bulk_32_blocks_round_trip() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(pipeline_wide_flush_config_fixed(path)).expect("open");
    let chain = build_chain(40);
    let slice: Vec<L2Block> = chain.into_iter().skip(4).take(32).collect();
    let mut acks = Vec::with_capacity(slice.len());
    for b in &slice {
        acks.push(
            store
                .put_pipelined(b.clone(), false)
                .await
                .expect("enqueue"),
        );
    }
    for rx in acks {
        assert!(rx.await.expect("ack").expect("inserted"));
    }
    for b in &slice {
        let g = store
            .get_block(&b.hash())
            .expect("sync get")
            .expect("present");
        assert_eq!(g.hash(), b.hash());
    }
}

/// **Proves:** BLK-008 AC §8 + spec test plan “graceful shutdown” — dropping the last [`BlockStore`] closes the
/// pipeline [`mpsc`] sender; the worker observes `recv(None)`, flushes its partial buffer, and exits. This test
/// enqueues **one** block with a huge flush window (so the worker would otherwise sit idle), drops the store **without**
/// awaiting the oneshot, then reopens read-only and asserts the block is durable on disk.
#[tokio::test]
async fn test_graceful_shutdown_flushes_unacked_partial_batch() {
    let (_guard, path) = temp_blockstore_dir();
    let path_ro = path.clone();
    {
        let store = BlockStore::open(BlockStoreConfig {
            path: path.clone(),
            write_pipeline_batch_size: 256,
            write_pipeline_flush_ms: 3_600_000,
            write_pipeline_channel_capacity: 8,
            ..test_config(path)
        })
        .expect("open");
        let b = test_block(1, ZERO_HASH);
        let _rx = store
            .put_pipelined(b.clone(), false)
            .await
            .expect("enqueue");
    }
    // The worker runs on the same runtime; dropping the last [`BlockStore`] closes the `mpsc` sender asynchronously
    // relative to this task. Yield + a short sleep so [`run_write_pipeline`] can observe `recv(None)`, call
    // [`flush_pipeline_batch`], and release the primary RocksDB handle before we open a second read-only handle.
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let ro = BlockStore::open_readonly(path_ro).expect("reopen ro");
    let b = test_block(1, ZERO_HASH);
    assert!(
        ro.get_block(&b.hash()).expect("get").is_some(),
        "partial batch must flush when ingress channel closes (pipeline worker shutdown)"
    );
}

/// **Proves:** [`BLK-008.md`](../docs/requirements/domains/block_storage/specs/BLK-008.md) test plan “backpressure” —
/// many concurrent producers share a **small** `write_pipeline_channel_capacity`; [`BlockStore::put_pipelined`]
/// awaits `mpsc::send` until capacity exists, so no block is dropped. After all tasks finish, every hash must
/// round-trip via [`BlockStore::get_block`].
#[tokio::test]
async fn test_bounded_channel_backpressure_without_loss() {
    let (_guard, path) = temp_blockstore_dir();
    let store = BlockStore::open(BlockStoreConfig {
        path: path.clone(),
        write_pipeline_batch_size: 64,
        // Short timer so the worker does not sit in [`tokio::select!`] for minutes while the bounded channel proves
        // backpressure (large `write_pipeline_flush_ms` would stall the first partial batch).
        write_pipeline_flush_ms: 250,
        write_pipeline_channel_capacity: 2,
        ..test_config(path)
    })
    .expect("open");
    let chain = build_chain(24);
    let slice: Vec<L2Block> = chain.into_iter().skip(3).take(12).collect();
    let mut join = Vec::new();
    for b in &slice {
        let st = store.clone();
        let b = b.clone();
        join.push(tokio::spawn(async move {
            let rx = st.put_pipelined(b.clone(), false).await.expect("enqueue");
            (b.hash(), rx.await.expect("oneshot join").expect("store op"))
        }));
    }
    for h in join {
        let (hash, inserted) = h.await.expect("task join");
        assert!(inserted, "each height must insert once (hash {hash:?})");
    }
    for b in &slice {
        assert!(
            store.get_block(&b.hash()).expect("get").is_some(),
            "hash {:?} must persist after congested ingress",
            b.hash()
        );
    }
}

/// **Proves:** Read-only stores reject pipeline enqueue with the same stable error surface as [`BlockStore::put_block`].
#[tokio::test]
async fn test_pipeline_read_only_rejected() {
    let (_guard, path) = temp_blockstore_dir();
    let path_ro = path.clone();
    {
        let s = BlockStore::open(test_config(path)).expect("open rw");
        let b = test_block(0, ZERO_HASH);
        s.put_block(&b, false).expect("seed");
    }
    let ro = BlockStore::open_readonly(path_ro).expect("ro");
    let b = test_block(1, ZERO_HASH);
    let err = ro
        .put_pipelined(b, false)
        .await
        .expect_err("read-only must error");
    match err {
        BlockStoreError::Serialization(s) => {
            assert!(s.contains("read-only") || s.contains("read only"), "{s}");
        }
        other => panic!("unexpected {other:?}"),
    }
}
