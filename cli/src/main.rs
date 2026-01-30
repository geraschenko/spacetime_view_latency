//! CLI for reproducing SpacetimeDB view latency scaling issue.
//!
//! ## Usage
//!
//! 1. Publish the module: `spacetime publish --project-path module view-latency`
//! 2. Run the test: `cargo run -p cli -- test --batch-size 100 --batches 10`
//!
//! ## What This Tests
//!
//! The test:
//! 1. Connects with a subscription to messages_view (filtered by sender identity)
//! 2. Sends messages in batches, measuring per-message reducer round-trip time
//! 3. Reports latency statistics per batch to show how latency grows with view size
//!
//! ## Expected Result (if bug exists)
//!
//! Latency per insert should grow linearly with the total number of messages.

mod generated;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, Subcommand};
use generated::append_message_reducer::append_message;
use generated::messages_view_table::MessagesViewTableAccess;
use generated::DbConnection;
use spacetimedb_sdk::{DbContext, Status, Table};

const SERVER: &str = "http://127.0.0.1:3000";
const DATABASE: &str = "view-latency";

#[derive(Parser)]
#[command(name = "cli")]
#[command(about = "Measures SpacetimeDB view subscription latency")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the latency measurement test
    Test {
        /// Number of messages per batch
        #[arg(long, default_value = "100")]
        batch_size: u64,

        /// Number of batches to send
        #[arg(long, default_value = "10")]
        batches: u64,

        /// Delay between batches in milliseconds
        #[arg(long, default_value = "2000")]
        batch_delay_ms: u64,

        /// Whether to subscribe to the view (vs no subscription)
        #[arg(long, default_value = "true")]
        subscribe: bool,
    },
    /// Run a single insert and measure latency
    Single,
    /// Watch for messages (debug mode)
    Watch,
}

/// Tracks pending inserts waiting for confirmation
struct PendingInserts {
    inserts: HashMap<u64, Instant>,
    next_id: u64,
    latencies: Vec<Duration>,
}

impl PendingInserts {
    fn new() -> Self {
        Self {
            inserts: HashMap::new(),
            next_id: 0,
            latencies: Vec::new(),
        }
    }

    fn start(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.inserts.insert(id, Instant::now());
        id
    }

    fn complete(&mut self, id: u64) -> Option<Duration> {
        if let Some(start) = self.inserts.remove(&id) {
            let latency = start.elapsed();
            self.latencies.push(latency);
            Some(latency)
        } else {
            None
        }
    }

    fn stats(&self) -> LatencyStats {
        if self.latencies.is_empty() {
            return LatencyStats::default();
        }

        let mut sorted: Vec<_> = self.latencies.iter().copied().collect();
        sorted.sort();

        let sum: Duration = sorted.iter().sum();
        let avg = sum / sorted.len() as u32;
        let min = sorted[0];
        let max = sorted[sorted.len() - 1];
        let p50 = sorted[sorted.len() / 2];
        let p99 = sorted[(sorted.len() as f64 * 0.99) as usize];

        LatencyStats { avg, min, max, p50, p99, count: sorted.len() }
    }

    fn clear_latencies(&mut self) {
        self.latencies.clear();
    }
}

#[derive(Default)]
struct LatencyStats {
    avg: Duration,
    min: Duration,
    max: Duration,
    p50: Duration,
    p99: Duration,
    count: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Test {
            batch_size,
            batches,
            batch_delay_ms,
            subscribe,
        } => cmd_test(batch_size, batches, batch_delay_ms, subscribe)?,
        Commands::Single => cmd_single()?,
        Commands::Watch => cmd_watch()?,
    }

    Ok(())
}

fn cmd_test(batch_size: u64, batches: u64, batch_delay_ms: u64, subscribe: bool) -> Result<()> {
    println!("=== View Latency Scaling Test ===");
    println!("Batch size: {}", batch_size);
    println!("Number of batches: {}", batches);
    println!("Batch delay: {}ms", batch_delay_ms);
    println!("Subscribe to view: {}", subscribe);
    println!();

    let pending = Arc::new(Mutex::new(PendingInserts::new()));
    let total_received = Arc::new(std::sync::atomic::AtomicU64::new(0));

    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
    let (batch_done_tx, batch_done_rx) = std::sync::mpsc::channel::<()>();

    // Build connection
    println!("[Client] Connecting...");

    let conn = DbConnection::builder()
        .with_uri(SERVER)
        .with_module_name(DATABASE)
        .on_connect({
            let ready_tx = ready_tx.clone();
            let total_received = total_received.clone();
            move |ctx, identity, _token| {
                println!("[Client] Connected as {:?}", identity);

                if subscribe {
                    // Register on_insert callback to count received messages
                    let total_received = total_received.clone();
                    ctx.db.messages_view().on_insert(move |_ctx, _row| {
                        total_received.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    });

                    // Subscribe to the view
                    ctx.subscription_builder()
                        .on_applied({
                            let ready_tx = ready_tx.clone();
                            move |_ctx| {
                                println!("[Client] Subscription applied");
                                let _ = ready_tx.send(());
                            }
                        })
                        .on_error(|_ctx, err| {
                            eprintln!("[Client] Subscription error: {:?}", err);
                        })
                        .subscribe(["SELECT * FROM messages_view"]);
                } else {
                    let _ = ready_tx.send(());
                }
            }
        })
        .on_connect_error(|_ctx, err| {
            eprintln!("[Client] Connection error: {:?}", err);
        })
        .build()?;

    // Track reducer confirmations with latency measurement
    let pending_for_reducer = pending.clone();
    let batch_size_for_reducer = batch_size;
    let batch_done_tx_for_reducer = batch_done_tx.clone();
    conn.reducers.on_append_message(move |ctx, _content| {
        if let Status::Committed = &ctx.event.status {
            // We use the content to extract the message id (hacky but works)
            // Actually, we'll track by sequential id
            let mut pending = pending_for_reducer.lock().unwrap();
            // The problem: we don't have the id here
            // Workaround: complete in order (FIFO assumption)
            if let Some((&oldest_id, _)) = pending.inserts.iter().min_by_key(|(id, _)| *id) {
                let _ = pending.complete(oldest_id);
            }

            // Signal when batch is complete
            if pending.latencies.len() as u64 >= batch_size_for_reducer {
                let _ = batch_done_tx_for_reducer.send(());
            }
        }
    });

    conn.run_threaded();

    // Wait for ready
    println!("[Client] Waiting for connection...");
    ready_rx.recv()?;
    println!("[Client] Ready!");
    println!();

    // Run batches
    let mut all_stats: Vec<(u64, LatencyStats)> = Vec::new();

    for batch in 1..=batches {
        let total_messages = batch * batch_size;
        println!("=== Batch {} (total messages: {}) ===", batch, total_messages);

        // Clear latency tracking for this batch
        {
            let mut pending = pending.lock().unwrap();
            pending.clear_latencies();
        }

        // Send batch
        let batch_start = Instant::now();
        for i in 0..batch_size {
            let content = format!("batch{}msg{}", batch, i);

            // Mark start time
            {
                let mut pending = pending.lock().unwrap();
                pending.start();
            }

            conn.reducers.append_message(content)?;
        }

        // Wait for all confirmations
        let _ = batch_done_rx.recv_timeout(Duration::from_secs(60));

        let batch_elapsed = batch_start.elapsed();

        // Get stats for this batch
        let stats = {
            let pending = pending.lock().unwrap();
            pending.stats()
        };

        println!(
            "  Batch time: {:?}, Confirmations: {}",
            batch_elapsed, stats.count
        );
        println!(
            "  Latency - avg: {:?}, min: {:?}, max: {:?}, p50: {:?}, p99: {:?}",
            stats.avg, stats.min, stats.max, stats.p50, stats.p99
        );

        let received = total_received.load(std::sync::atomic::Ordering::SeqCst);
        println!("  on_insert callbacks received: {}", received);
        println!();

        all_stats.push((total_messages, stats));

        if batch < batches {
            std::thread::sleep(Duration::from_millis(batch_delay_ms));
        }
    }

    // Summary
    println!("=== SUMMARY ===");
    println!("{:>10} {:>12} {:>12} {:>12}", "Messages", "Avg (ms)", "P50 (ms)", "P99 (ms)");
    for (total, stats) in &all_stats {
        println!(
            "{:>10} {:>12.2} {:>12.2} {:>12.2}",
            total,
            stats.avg.as_secs_f64() * 1000.0,
            stats.p50.as_secs_f64() * 1000.0,
            stats.p99.as_secs_f64() * 1000.0,
        );
    }

    // Check for linear growth
    if all_stats.len() >= 2 {
        let first = &all_stats[0];
        let last = &all_stats[all_stats.len() - 1];

        let latency_ratio = last.1.avg.as_secs_f64() / first.1.avg.as_secs_f64();
        let message_ratio = last.0 as f64 / first.0 as f64;

        println!();
        println!("Latency growth: {:.2}x over {:.2}x more messages", latency_ratio, message_ratio);

        if latency_ratio > message_ratio * 0.5 {
            println!("STATUS: View latency appears to scale with message count!");
            println!("This confirms the O(N) view materialization hypothesis.");
        } else {
            println!("STATUS: Latency growth is sub-linear. View materialization may not be the bottleneck.");
        }
    }

    Ok(())
}

fn cmd_single() -> Result<()> {
    println!("Sending single message and measuring latency...");

    let (done_tx, done_rx) = std::sync::mpsc::channel::<Duration>();
    let start = Arc::new(Mutex::new(None::<Instant>));

    let conn = DbConnection::builder()
        .with_uri(SERVER)
        .with_module_name(DATABASE)
        .on_connect({
            let start = start.clone();
            move |ctx, identity, _token| {
                println!("Connected as {:?}", identity);

                ctx.subscription_builder()
                    .on_applied({
                        let start = start.clone();
                        move |ctx| {
                            println!("Subscription applied, sending message...");
                            *start.lock().unwrap() = Some(Instant::now());
                            let _ = ctx.reducers.append_message("test".to_string());
                        }
                    })
                    .subscribe(["SELECT * FROM messages_view"]);
            }
        })
        .on_connect_error(|_ctx, err| {
            eprintln!("Connection error: {:?}", err);
        })
        .build()?;

    let start_for_reducer = start.clone();
    let done_tx_for_reducer = done_tx.clone();
    conn.reducers.on_append_message(move |ctx, _content| {
        if let Status::Committed = &ctx.event.status {
            if let Some(start) = start_for_reducer.lock().unwrap().take() {
                let _ = done_tx_for_reducer.send(start.elapsed());
            }
        }
    });

    conn.run_threaded();

    match done_rx.recv_timeout(Duration::from_secs(30)) {
        Ok(latency) => {
            println!("Round-trip latency: {:?}", latency);
        }
        Err(_) => {
            eprintln!("Timeout waiting for confirmation");
        }
    }

    Ok(())
}

fn cmd_watch() -> Result<()> {
    println!("Watching for messages...");
    println!("Press Ctrl+C to exit.");
    println!();

    let conn = DbConnection::builder()
        .with_uri(SERVER)
        .with_module_name(DATABASE)
        .on_connect(move |ctx, identity, _token| {
            println!("Connected as {:?}", identity);

            ctx.db.messages_view().on_insert(move |_ctx, row| {
                println!(
                    "on_insert: id={}, ts={:?}, content={}",
                    row.id, row.ts, row.content
                );
            });

            ctx.subscription_builder()
                .on_applied(|_ctx| {
                    println!("Subscription applied, waiting for inserts...");
                })
                .on_error(|_ctx, err| {
                    eprintln!("Subscription error: {:?}", err);
                })
                .subscribe(["SELECT * FROM messages_view"]);
        })
        .on_connect_error(|_ctx, err| {
            eprintln!("Connection error: {:?}", err);
        })
        .build()?;

    conn.run_threaded();

    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
