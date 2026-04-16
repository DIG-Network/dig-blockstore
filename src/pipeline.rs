//! Async batched write pipeline and canonical range streaming
//! ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md),
//! [`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md)).
//!
//! # Scope
//!
//! - [`PipelineJob`] — one ingress unit (block + canonical flag + ack channel).
//! - [`BlockStore::put_pipelined`] — enqueue a block for batched write; returns a
//!   [`oneshot::Receiver`] that resolves when the worker has committed the batch.
//! - [`BlockStore::pipeline_write_batch_count`] — [`WriteBatch`] commit counter.
//! - [`run_write_pipeline`] — background task that drains the mpsc channel into
//!   one [`WriteBatch`] per flush interval.
//! - [`flush_pipeline_batch`] — single-batch commit mirroring [`BlockStore::put_block`] semantics.
//! - [`StreamBlocksInRange`] — readahead-backed iterator over canonical bodies
//!   for a closed height range.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use chia_protocol::Bytes32;
use dig_block::{BlockStatus, L2Block};
use rocksdb::{ColumnFamily, Direction, IteratorMode, ReadOptions, WriteBatch};
use tokio::sync::{mpsc, oneshot};

use crate::constants::{CF_BLOCKS, CF_CANONICAL, CF_HEADERS};
use crate::encoding::{decode_height_key, hash_key, height_key};
use crate::error::{BlockStoreError, ERR_MUTATION_READ_ONLY};
use crate::store::{BlockStore, BlockStoreInner};
use crate::types::BlockRecord;

/// One ingress job for [`run_write_pipeline`]: own the [`L2Block`], canonical flag, and per-block ack channel
/// ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md) + [`IMPLEMENTATION_ORDER.md`](../docs/requirements/IMPLEMENTATION_ORDER.md) Phase 5).
///
/// **Ack semantics:** `Ok(true)` means a **new** row was written to [`CF_BLOCKS`]; `Ok(false)` matches [`BlockStore::put_block`]
/// idempotency (duplicate hash on disk or duplicate within the same batch).
pub(crate) type PipelineJob = (
    L2Block,
    bool,
    oneshot::Sender<Result<bool, BlockStoreError>>,
);

impl BlockStore {
    /// Build [`ReadOptions`] for sequential [`CF_BLOCKS`] reads inside [`StreamBlocksInRange`] ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md) AC §3).
    fn blocks_stream_read_options(&self) -> ReadOptions {
        let mut o = ReadOptions::default();
        o.set_readahead_size(self.readahead_size);
        o
    }

    /// Stream canonical blocks from height `start` through `end` inclusive ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md)).
    ///
    /// **Phase 1 — canonical walk:** [`rocksdb::DB::iterator_cf_opt`] over [`CF_CANONICAL`] with
    /// [`ReadOptions::set_readahead_size`] and iterate bounds
    /// ([`KEY-002`](../docs/requirements/domains/key_encoding/specs/KEY-002_height_keys.md) big-endian order).
    ///
    /// **Phase 2 — lazy bodies:** The returned [`StreamBlocksInRange`] walks the captured `(height, hash)` slice and,
    /// for each entry, serves [`ShardedBlockCache`](crate::cache::sharded::ShardedBlockCache) hits without RocksDB, or
    /// [`rocksdb::DB::get_cf_opt`] on [`CF_BLOCKS`] with the same readahead hint (separate [`ReadOptions`]
    /// instance so canonical and block reads each carry the configured hint).
    ///
    /// **Why two phases:** A live RocksDB iterator over [`CF_CANONICAL`] cannot coexist with mutable/immutable borrows
    /// of `block_cache` / decompressors on every `Iterator::next` without self-referential structs; materializing
    /// the height→hash list preserves **readahead on the canonical scan** while keeping the public API safe and `'static`-free.
    ///
    /// **Errors:** Missing [`CF_BLOCKS`] row for a canonical hash yields [`BlockStoreError::BlockNotFound`] from the stream
    /// ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md) AC §6). Malformed canonical keys/values map to [`BlockStoreError::Serialization`].
    ///
    /// **Empty / inverted range:** If `start > end`, returns an iterator that yields immediately without I/O.
    pub fn stream_blocks_in_range(
        &self,
        start: u64,
        end: u64,
    ) -> Result<StreamBlocksInRange<'_>, BlockStoreError> {
        let cf_blocks = self.cf(CF_BLOCKS)?;
        if start > end {
            return Ok(StreamBlocksInRange {
                store: self,
                pairs: Vec::new(),
                idx: 0,
                read_opts: self.blocks_stream_read_options(),
                cf_blocks,
            });
        }
        let cf_canon = self.cf(CF_CANONICAL)?;
        let mut ro_canon = ReadOptions::default();
        ro_canon.set_readahead_size(self.readahead_size);
        ro_canon.set_iterate_lower_bound(height_key(start).to_vec());
        if end < u64::MAX {
            ro_canon.set_iterate_upper_bound(height_key(end.saturating_add(1)).to_vec());
        }
        let iter = self.db.iterator_cf_opt(
            cf_canon,
            ro_canon,
            IteratorMode::From(height_key(start).as_slice(), Direction::Forward),
        );
        let mut pairs = Vec::new();
        for item in iter {
            let (k, v) = item?;
            let karr: [u8; 8] = k.as_ref().try_into().map_err(|_| {
                BlockStoreError::Serialization(
                    "stream_blocks_in_range: CF_CANONICAL key must be exactly 8 bytes".into(),
                )
            })?;
            let height = decode_height_key(&karr);
            if height > end {
                break;
            }
            if height < start {
                continue;
            }
            let varr: [u8; 32] = v.as_ref().try_into().map_err(|_| {
                BlockStoreError::Serialization(
                    "stream_blocks_in_range: CF_CANONICAL value must be exactly 32 bytes".into(),
                )
            })?;
            pairs.push((height, Bytes32::new(varr)));
        }
        Ok(StreamBlocksInRange {
            store: self,
            pairs,
            idx: 0,
            read_opts: self.blocks_stream_read_options(),
            cf_blocks,
        })
    }

    /// Async batched ingest ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md), [`IMPLEMENTATION_ORDER.md`](../docs/requirements/IMPLEMENTATION_ORDER.md) Phase 5).
    ///
    /// **Channel + batching (NORMATIVE §1–3):** Enqueues into a bounded [`mpsc`] queue; a background task accumulates
    /// up to `pipeline_batch_size` jobs or until `pipeline_flush_ms` elapses,
    /// then applies **one** [`WriteBatch`] mirroring [`BlockStore::put_block`] semantics.
    ///
    /// **Per-block ack ([`IMPLEMENTATION_ORDER.md`](../docs/requirements/IMPLEMENTATION_ORDER.md)):** The returned
    /// [`oneshot::Receiver`] resolves to the same `Result<bool, BlockStoreError>` shape as [`BlockStore::put_block`]
    /// (`Ok(true)` inserted, `Ok(false)` duplicate).
    ///
    /// **Runtime contract:** The first call lazily spawns [`run_write_pipeline`] via [`tokio::spawn`]; therefore an
    /// active [`tokio::runtime::Handle`] must exist (integration tests should use `#[tokio::test]`).
    pub async fn put_pipelined(
        &self,
        block: L2Block,
        canonical: bool,
    ) -> Result<oneshot::Receiver<Result<bool, BlockStoreError>>, BlockStoreError> {
        if self.read_only {
            return Err(BlockStoreError::Serialization(
                ERR_MUTATION_READ_ONLY.into(),
            ));
        }
        let tx = self.pipeline_sender().await?;
        let (ack_tx, ack_rx) = oneshot::channel();
        tx.send((block, canonical, ack_tx))
            .await
            .map_err(|_| BlockStoreError::PipelineClosed)?;
        Ok(ack_rx)
    }

    /// Count of successful RocksDB [`WriteBatch`] commits executed by the [`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md) worker.
    ///
    /// **Instrumentation:** Used by `tests/blk_008_tests.rs` to prove AC §4 "single `WriteBatch` per flush interval".
    #[must_use]
    pub fn pipeline_write_batch_count(&self) -> u64 {
        self.pipeline_write_batches.load(Ordering::Relaxed) as u64
    }

    /// Lazily constructs the bounded [`mpsc`] sender and spawns [`run_write_pipeline`].
    async fn pipeline_sender(&self) -> Result<mpsc::Sender<PipelineJob>, BlockStoreError> {
        let mut guard = self.pipeline_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            return Ok(tx.clone());
        }
        let _handle = tokio::runtime::Handle::try_current().map_err(|_| {
            BlockStoreError::Serialization(
                "put_pipelined requires an active Tokio runtime (use #[tokio::test] or Runtime::block_on)"
                    .into(),
            )
        })?;
        let cap = self.pipeline_channel_capacity;
        let (tx, rx) = mpsc::channel::<PipelineJob>(cap);
        let inner = self.inner.clone();
        let batch = self.pipeline_batch_size;
        let flush_ms = self.pipeline_flush_ms;
        tokio::spawn(run_write_pipeline(
            inner,
            Arc::new(tokio::sync::Mutex::new(None)),
            rx,
            batch,
            flush_ms,
        ));
        *guard = Some(tx.clone());
        Ok(tx)
    }
}

/// Background loop draining [`PipelineJob`] values into batched [`WriteBatch`] commits ([`BLK-008`](../docs/requirements/domains/block_storage/specs/BLK-008.md)).
///
/// **Shutdown (AC §8):** Ingress senders live on [`BlockStore::pipeline_tx`], not on [`BlockStoreInner`]. When the last
/// [`BlockStore`] clone is dropped, the final [`mpsc::Sender`] is released, `rx.recv()` yields `None`, and we
/// [`flush_pipeline_batch`] any tail buffer before exiting.
pub(crate) async fn run_write_pipeline(
    inner: Arc<BlockStoreInner>,
    _worker_unused_pipeline_tx: Arc<tokio::sync::Mutex<Option<mpsc::Sender<PipelineJob>>>>,
    mut rx: mpsc::Receiver<PipelineJob>,
    batch_size: usize,
    flush_ms: u64,
) {
    let store = BlockStore {
        inner,
        pipeline_tx: _worker_unused_pipeline_tx,
    };
    let mut buf: Vec<PipelineJob> = Vec::with_capacity(batch_size);
    let tick = Duration::from_millis(flush_ms);

    loop {
        match rx.recv().await {
            None => {
                return;
            }
            Some(job) => buf.push(job),
        }
        if buf.len() >= batch_size {
            let _ = flush_pipeline_batch(&store, &mut buf);
            buf.clear();
            continue;
        }

        let mut sleep = Box::pin(tokio::time::sleep(tick));
        'collect: loop {
            tokio::select! {
                biased;
                maybe = rx.recv() => {
                    match maybe {
                        None => {
                            let _ = flush_pipeline_batch(&store, &mut buf);
                            return;
                        }
                        Some(job) => {
                            buf.push(job);
                            if buf.len() >= batch_size {
                                break 'collect;
                            }
                        }
                    }
                }
                _ = &mut sleep, if !buf.is_empty() => {
                    break 'collect;
                }
            }
        }

        let _ = flush_pipeline_batch(&store, &mut buf);
        buf.clear();
    }
}

/// Applies one RocksDB [`WriteBatch`] for all novel inserts in `jobs`, mirroring [`BlockStore::put_block`].
///
/// **Idempotency (AC §5):** Duplicate hashes already on disk **or** repeated within `jobs` are answered with
/// `Ok(false)` acks and are omitted from the write batch. A completely duplicate batch performs **no** `db.write`.
///
/// **Errors:** Build/IO failures notify every still-pending staged [`oneshot`] with [`BlockStoreError::Serialization`]
/// carrying the diagnostic string, then the function returns `Ok(())` so the worker loop keeps draining (best-effort
/// bulk ingest semantics; callers observe failure on their ack channel).
fn flush_pipeline_batch(
    store: &BlockStore,
    jobs: &mut Vec<PipelineJob>,
) -> Result<(), BlockStoreError> {
    if store.read_only {
        let pending: Vec<PipelineJob> = std::mem::take(jobs);
        let msg = ERR_MUTATION_READ_ONLY.to_string();
        for (_, _, ack) in pending {
            let _ = ack.send(Err(BlockStoreError::Serialization(msg.clone())));
        }
        return Ok(());
    }
    let pending: Vec<PipelineJob> = std::mem::take(jobs);
    let cf_b = match store.cf(CF_BLOCKS) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            for (_, _, ack) in pending {
                let _ = ack.send(Err(BlockStoreError::Serialization(format!(
                    "write pipeline: {msg}"
                ))));
            }
            return Ok(());
        }
    };
    let cf_h = match store.cf(CF_HEADERS) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            for (_, _, ack) in pending {
                let _ = ack.send(Err(BlockStoreError::Serialization(format!(
                    "write pipeline: {msg}"
                ))));
            }
            return Ok(());
        }
    };
    let cf_c = match store.cf(CF_CANONICAL) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            for (_, _, ack) in pending {
                let _ = ack.send(Err(BlockStoreError::Serialization(format!(
                    "write pipeline: {msg}"
                ))));
            }
            return Ok(());
        }
    };

    let mut seen: HashSet<Bytes32> = HashSet::new();
    struct StagedRow {
        hash: Bytes32,
        block: L2Block,
        compressed: Vec<u8>,
        header_bytes: Vec<u8>,
        canonical: bool,
        ack: oneshot::Sender<Result<bool, BlockStoreError>>,
    }
    let mut staged: Vec<StagedRow> = Vec::new();

    for (block, canonical, ack) in pending {
        let hash = block.hash();
        if !seen.insert(hash) {
            let _ = ack.send(Ok(false));
            continue;
        }
        let exists = match store.db.get_cf(cf_b, hash_key(&hash).as_slice()) {
            Ok(o) => o.is_some(),
            Err(e) => {
                let _ = ack.send(Err(BlockStoreError::RocksDb(e)));
                continue;
            }
        };
        if exists {
            let _ = ack.send(Ok(false));
            continue;
        }
        let compressed = match store.serialize_block(&block) {
            Ok(b) => b,
            Err(e) => {
                let _ = ack.send(Err(e));
                continue;
            }
        };
        let header_bytes = match BlockStore::serialize_header(&block.header) {
            Ok(b) => b,
            Err(e) => {
                let _ = ack.send(Err(e));
                continue;
            }
        };
        staged.push(StagedRow {
            hash,
            block,
            compressed,
            header_bytes,
            canonical,
            ack,
        });
    }

    let mut wb = WriteBatch::default();
    for row in &staged {
        wb.put_cf(
            cf_b,
            hash_key(&row.hash).as_slice(),
            row.compressed.as_slice(),
        );
        wb.put_cf(
            cf_h,
            hash_key(&row.hash).as_slice(),
            row.header_bytes.as_slice(),
        );
        if row.canonical {
            wb.put_cf(
                cf_c,
                height_key(row.block.height()),
                hash_key(&row.hash).as_slice(),
            );
        }
    }

    if wb.is_empty() {
        return Ok(());
    }

    if let Err(e) = store.db.write(wb) {
        let msg = format!("write pipeline: rocksdb write failed: {e}");
        for row in staged {
            let _ = row
                .ack
                .send(Err(BlockStoreError::Serialization(msg.clone())));
        }
        return Ok(());
    }

    for row in &staged {
        if row.canonical {
            if let Err(e) = store
                .canonical_bin
                .write()
                .extend_write(row.block.height(), &row.hash)
            {
                let msg = format!("write pipeline: canonical.bin mmap update failed: {e}");
                for row in staged {
                    let _ = row
                        .ack
                        .send(Err(BlockStoreError::Serialization(msg.clone())));
                }
                return Ok(());
            }
        }
    }

    store.pipeline_write_batches.fetch_add(1, Ordering::Relaxed);

    for row in staged {
        let record = BlockRecord::from_header(&row.block.header, BlockStatus::Validated);
        store.record_cache.lock().insert(row.hash, record);
        store.block_cache.insert(row.hash, row.block.clone());
        store
            .header_cache
            .insert(row.hash, row.block.header.clone());
        let ack_res = match store.maybe_train_dictionary() {
            Ok(()) => Ok(true),
            Err(e) => Err(e),
        };
        let _ = row.ack.send(ack_res);
    }
    Ok(())
}

/// Lazy iterator over canonical block bodies for a closed height range ([`BLK-006`](../docs/requirements/domains/block_storage/specs/BLK-006.md)).
///
/// Constructed only via [`BlockStore::stream_blocks_in_range`]. Holds a precomputed `(height, hash)` list from a
/// readahead-backed scan of [`CF_CANONICAL`], then loads [`CF_BLOCKS`] rows on demand so callers can stop early without
/// decompressing the remainder ([`BLK-006.md`](../docs/requirements/domains/block_storage/specs/BLK-006.md) implementation notes).
///
/// **Invariants:** Heights in `pairs` are strictly ascending (RocksDB canonical ordering). Each successful item matches
/// the canonical hash at that height; [`L2Block::height`](dig_block::L2Block::height) should equal the stored height
/// when the database is consistent ([`BLK-001`](../docs/requirements/domains/block_storage/specs/BLK-001.md) write path).
pub struct StreamBlocksInRange<'a> {
    store: &'a BlockStore,
    pairs: Vec<(u64, Bytes32)>,
    idx: usize,
    read_opts: ReadOptions,
    cf_blocks: &'a ColumnFamily,
}

impl<'a> Iterator for StreamBlocksInRange<'a> {
    type Item = Result<L2Block, BlockStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.pairs.len() {
            return None;
        }
        let (_expected_height, hash) = self.pairs[self.idx];
        self.idx += 1;
        if let Some(block) = self.store.block_cache.get_clone(&hash) {
            return Some(Ok(block));
        }
        self.store
            .cf_blocks_stream_physical_gets
            .fetch_add(1, Ordering::Relaxed);
        let raw_opt = match self.store.db.get_cf_opt(
            self.cf_blocks,
            hash_key(&hash).as_slice(),
            &self.read_opts,
        ) {
            Ok(o) => o,
            Err(e) => return Some(Err(e.into())),
        };
        let Some(raw) = raw_opt else {
            return Some(Err(BlockStoreError::BlockNotFound(hash)));
        };
        match self.store.deserialize_block(&raw) {
            Ok(block) => {
                self.store.block_cache.insert(hash, block.clone());
                self.store.header_cache.insert(hash, block.header.clone());
                Some(Ok(block))
            }
            Err(e) => Some(Err(e)),
        }
    }
}
