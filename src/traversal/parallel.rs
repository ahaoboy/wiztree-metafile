// Parallel traversal strategy.
//
// The previous "multi-threaded" mode installed a rayon pool but ran the
// exact same single-threaded traversal inside `pool.install()`, so
// `thread_count > 1` had no effect at all. This module replaces that with a
// real worker pool that steals work from one another:
//
//  * A shared `Injector<T>` crossbeam deque is seeded with the root
//    directory.
//  * Each worker owns a local `Worker<T>` queue (LIFO for DFS flavour, FIFO
//    for BFS flavour). New tasks produced by a worker are pushed onto its
//    own local queue first; idle workers pull from the injector, then try
//    to steal from other workers' queues.
//  * An `AtomicUsize` "active" counter tracks the number of unfinished dir
//    tasks across all workers; reaching zero naturally drains the queue and
//    signals shutdown. A separate `AtomicBool` "stop" flag triggers early
//    shutdown when `max_files` is reached.
//  * Each worker accumulates files into its own private `LocalResult` (a
//    plain `Vec<FileEntry>` plus counters); all of the workers' results are
//    merged in the driver thread at the end. **Zero locking on the hot
//    path** — the only `Mutex` touched per file is the inode dedup set,
//    which is unavoidable for correctness across worker threads.

use crate::analyzer::LocalResult;
use crate::config::{AnalyzerConfig, TraversalStrategy};
use crate::error::AnalyzerError;
use crate::link_handler::LinkHandler;
use crate::traversal::helpers::{self, TraverseCtx};
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use rayon::ThreadPoolBuildError;
use rayon::{ThreadPool, ThreadPoolBuilder, scope};
use std::mem;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Work-stealing task unit.
#[derive(Debug)]
struct Task {
    path: std::path::PathBuf,
    depth: usize,
    /// When `true`, this task is the root directory itself (avoids re-counting
    /// it on first pop since `bootstrap_root` already accounted for it).
    is_root_dir: bool,
}

impl Task {
    fn new(path: std::path::PathBuf, depth: usize, is_root_dir: bool) -> Self {
        Self {
            path,
            depth,
            is_root_dir,
        }
    }
}

pub fn traverse(
    config: &AnalyzerConfig,
    link_handler: &Arc<LinkHandler>,
) -> Result<LocalResult, AnalyzerError> {
    // Build a rayon pool sized to `config.thread_count` so the number of OS
    // threads respects the user's flag regardless of RAYON_NUM_THREADS.
    let pool: ThreadPool = ThreadPoolBuilder::new()
        .num_threads(config.thread_count)
        .build()
        .map_err(map_pool_err)?;

    let config_arc = Arc::new(config.clone());
    let link_handler = link_handler.clone();

    // Shared cross-thread state.
    let injector = Arc::new(Injector::new());
    let stop = Arc::new(AtomicBool::new(false));
    let active = Arc::new(AtomicUsize::new(0));

    // Account for the root directory before any worker yields anything: this
    // keeps the directory count consistent with the single-threaded driver.
    let mut seed_result = LocalResult::default();
    {
        let ctx = TraverseCtx::new(&config_arc, &link_handler);
        helpers::bootstrap_root(&ctx, &config_arc.root_path, &mut seed_result)?;
    }

    injector.push(Task::new(config_arc.root_path.clone(), 1, true));
    active.fetch_add(1, Ordering::SeqCst);

    let is_fifo = matches!(config.traversal_strategy, TraversalStrategy::BreadthFirst);

    // Per-worker local queues; stealers shared with all workers.
    // crossbeam-deque's `Worker<T>` is single-threaded (it is not `Sync`),
    // so each spawned closure MOVES one worker into itself; we do not need
    // an `Arc<Worker<...>>` (which would require `Worker: Sync`).
    let workers: Vec<Worker<Task>> = (0..config.thread_count)
        .map(|_| {
            if is_fifo {
                Worker::new_fifo()
            } else {
                Worker::new_lifo()
            }
        })
        .collect();
    let stealers: Vec<Stealer<Task>> = workers.iter().map(|w| w.stealer()).collect();

    // We can't get a `ScopedJoinHandle<R>` from rayon-core 1.13's `spawn`
    // (it returns `()`), so workers deposit their private `LocalResult` into
    // this shared slot. Each worker only takes the lock exactly once at the
    // very end of its run, so this is not on the hot path.
    let collected: Arc<Mutex<Vec<LocalResult>>> =
        Arc::new(Mutex::new(Vec::with_capacity(config.thread_count)));

    pool.install(|| {
        scope(|s| {
            // Move the per-worker `Worker` into its closure. The remaining
            // workers are temporarily held here so they keep their stealers
            // valid; we drain them after the loop.
            let mut workers_iter = workers.into_iter();
            for worker in workers_iter.by_ref() {
                let injector = injector.clone();
                let stealers = stealers.clone();
                let stop = stop.clone();
                let active = active.clone();
                let link_handler = link_handler.clone();
                let config_arc = config_arc.clone();
                let collected = collected.clone();

                s.spawn(move |_| {
                    let local = worker_run(
                        worker,
                        injector,
                        stealers,
                        stop,
                        active,
                        config_arc,
                        link_handler,
                    );
                    // Move the local result out of the worker thread, into
                    // the shared slot, with a single lock.
                    if let Ok(mut slot) = collected.lock() {
                        slot.push(local);
                    }
                });
            }
        });
    });

    // Pull out the accumulated results. The Arc is uniquely owned here
    // because all the worker clones have been dropped (their `spawn`
    // closures donated the clones and finished). Fall back to cloning the
    // storage if some clone is unexpectedly still alive.
    let mut merged = seed_result;
    let local_results = match Arc::try_unwrap(collected) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => mem::take(&mut *arc.lock().expect("results mutex poisoned")),
    };
    for local in local_results {
        merged.merge(local);
    }

    Ok(merged)
}

fn worker_run(
    worker: Worker<Task>,
    injector: Arc<Injector<Task>>,
    stealers: Vec<Stealer<Task>>,
    stop: Arc<AtomicBool>,
    active: Arc<AtomicUsize>,
    config: Arc<AnalyzerConfig>,
    link_handler: Arc<LinkHandler>,
) -> LocalResult {
    let mut result = LocalResult::default();
    let ctx = TraverseCtx::new(&config, &link_handler);

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        match find_task(&worker, &injector, &stealers) {
            Some(task) => run_task(&ctx, task, &worker, &active, &stop, &mut result),
            None => {
                if active.load(Ordering::Acquire) == 0 {
                    break;
                }
                std::thread::yield_now();
            }
        }
    }
    result
}

fn run_task(
    ctx: &TraverseCtx,
    task: Task,
    worker: &Worker<Task>,
    active: &AtomicUsize,
    stop: &AtomicBool,
    result: &mut LocalResult,
) {
    // The canonicalize happens in `helpers::process_entry` (called from the
    // parent's loop) BEFORE a Task is queued here. The root task's directory
    // was likewise visited via `bootstrap_root`. So by the time a worker
    // picks a task up off the deque, the path is ALREADY in
    // `visited_paths` — and we must NOT call `visit_dir` again here, or the
    // harmless second visit would be reported as a "Circular" warning and
    // the children of this directory would never be processed.
    let depth = if task.is_root_dir { 1 } else { task.depth };

    // Descend: list this directory's entries, and for each one process it.
    let entries = match ctx.walker.read_dir(&task.path, depth, ctx.limits.max_depth) {
        Ok(e) => e,
        Err(e) => {
            result.add_warning(format!(
                "Cannot read directory {}: {}",
                task.path.display(),
                e
            ));
            active.fetch_sub(1, Ordering::SeqCst);
            return;
        }
    };

    for entry in entries {
        if stop.load(Ordering::Acquire) {
            break;
        }
        if ctx.reached_max_files(result) {
            stop.store(true, Ordering::Release);
            break;
        }

        match helpers::process_entry(ctx, &entry.path, entry.file_type, entry.depth, result) {
            Ok(Some((dir, child_depth))) => {
                active.fetch_add(1, Ordering::SeqCst);
                worker.push(Task::new(dir, child_depth, false));
            }
            Ok(None) => {}
            Err(e) => {
                result.add_warning(format!(
                    "Cannot process entry {}: {}",
                    entry.path.display(),
                    e
                ));
            }
        }
    }

    active.fetch_sub(1, Ordering::SeqCst);
}

/// Find a task for the current worker in the following order:
///   1. own local queue (`worker.pop()`)
///   2. shared injector (`injector.steal()` loop on `Retry`)
///   3. other workers (`stealers[i].steal()`)
fn find_task(
    worker: &Worker<Task>,
    injector: &Injector<Task>,
    stealers: &[Stealer<Task>],
) -> Option<Task> {
    if let Some(t) = worker.pop() {
        return Some(t);
    }
    loop {
        match injector.steal() {
            Steal::Empty => break,
            Steal::Success(t) => return Some(t),
            Steal::Retry => continue,
        }
    }
    for stealer in stealers {
        match stealer.steal() {
            Steal::Empty => continue,
            Steal::Success(t) => return Some(t),
            Steal::Retry => continue,
        }
    }
    None
}

fn map_pool_err(e: ThreadPoolBuildError) -> AnalyzerError {
    AnalyzerError::ThreadPool(e.to_string())
}
