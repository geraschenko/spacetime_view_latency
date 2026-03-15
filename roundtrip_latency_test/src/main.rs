mod generated;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use generated::append_message_reducer::append_message;
use generated::DbConnection;
use spacetimedb_sdk::DbContext;

const DATABASE: &str = "view-latency";
const BATCH_SIZE: u64 = 100;
const NUM_BATCHES: u64 = 10;
const BATCH_DELAY_MS: u64 = 100;

#[derive(Parser)]
struct Cli {
    /// What to subscribe to: view, table, or none
    #[arg(long, value_enum, default_value = "view")]
    subscribe_to: SubscribeTo,

    /// Server URL
    #[arg(long, default_value = "http://127.0.0.1:3000")]
    server: String,

    /// Disable confirmed reads (reduces latency but sacrifices durability guarantees)
    #[arg(long)]
    no_confirmed_reads: bool,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum SubscribeTo {
    /// Subscribe to messages_view
    View,
    /// Subscribe directly to messages table
    Table,
    /// No subscription (baseline reducer latency)
    None,
}

/// Tracks reducer round-trip latencies
struct RoundtripTimer {
    latencies: Vec<Duration>,
}

impl RoundtripTimer {
    fn new() -> Self {
        Self {
            latencies: Vec::new(),
        }
    }

    fn record(&mut self, duration: Duration) {
        self.latencies.push(duration);
    }

    fn stats(&self) -> LatencyStats {
        if self.latencies.is_empty() {
            return LatencyStats::default();
        }

        let mut sorted: Vec<_> = self.latencies.iter().copied().collect();
        sorted.sort();

        let sum: Duration = sorted.iter().sum();
        let avg = sum / sorted.len() as u32;
        let p50 = sorted[sorted.len() / 2];
        let p99 = sorted[(sorted.len() as f64 * 0.99) as usize];

        LatencyStats { avg, p50, p99 }
    }

    fn clear(&mut self) {
        self.latencies.clear();
    }

    fn completed_count(&self) -> usize {
        self.latencies.len()
    }
}

#[derive(Default)]
struct LatencyStats {
    avg: Duration,
    p50: Duration,
    p99: Duration,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run_test(&cli)
}

fn run_test(cli: &Cli) -> Result<()> {
    let subscribe_to = cli.subscribe_to;

    println!("=== View Latency Scaling Test ===");
    println!("Server: {}", cli.server);
    println!("Subscribe to: {:?}", subscribe_to);
    println!("Confirmed reads: {}", !cli.no_confirmed_reads);
    println!("Batch size: {}, Batches: {}", BATCH_SIZE, NUM_BATCHES);
    println!();

    let roundtrip_timer = Arc::new(Mutex::new(RoundtripTimer::new()));

    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
    let (batch_done_tx, batch_done_rx) = std::sync::mpsc::channel::<()>();

    // Build connection
    let mut builder = DbConnection::builder()
        .with_uri(&cli.server)
        .with_database_name(DATABASE);

    if cli.no_confirmed_reads {
        builder = builder.with_confirmed_reads(false);
    }

    let conn = builder
        .on_connect({
            let ready_tx = ready_tx.clone();
            move |ctx, _identity, _token| match subscribe_to {
                SubscribeTo::View => {
                    ctx.subscription_builder()
                        .on_applied({
                            let ready_tx = ready_tx.clone();
                            move |_ctx| {
                                println!("Subscription applied (view)");
                                let _ = ready_tx.send(());
                            }
                        })
                        .on_error(|_ctx, err| {
                            eprintln!("Subscription error: {:?}", err);
                        })
                        .subscribe(["SELECT * FROM messages_view"]);
                }
                SubscribeTo::Table => {
                    ctx.subscription_builder()
                        .on_applied({
                            let ready_tx = ready_tx.clone();
                            move |_ctx| {
                                println!("Subscription applied (table)");
                                let _ = ready_tx.send(());
                            }
                        })
                        .on_error(|_ctx, err| {
                            eprintln!("Subscription error: {:?}", err);
                        })
                        .subscribe(["SELECT * FROM messages"]);
                }
                SubscribeTo::None => {
                    println!("No subscription");
                    let _ = ready_tx.send(());
                }
            }
        })
        .on_connect_error(|_ctx, err| {
            eprintln!("Connection error: {:?}", err);
        })
        .build()?;

    conn.run_threaded();

    ready_rx.recv()?;
    println!();

    // Run batches
    let mut all_stats: Vec<(u64, LatencyStats)> = Vec::new();

    for batch in 1..=NUM_BATCHES {
        std::thread::sleep(Duration::from_millis(BATCH_DELAY_MS));
        let total_messages = batch * BATCH_SIZE;

        {
            let mut timer = roundtrip_timer.lock().unwrap();
            timer.clear();
        }

        for i in 0..BATCH_SIZE {
            let content = format!("batch{}_message{}", batch, i);
            let start = Instant::now();
            conn.reducers.append_message_then(content, {
                let timer = roundtrip_timer.clone();
                let done_tx = batch_done_tx.clone();
                move |_ctx, result| match result {
                    Ok(Ok(())) => {
                        let mut t = timer.lock().unwrap();
                        t.record(start.elapsed());
                        if t.completed_count() as u64 >= BATCH_SIZE {
                            let _ = done_tx.send(());
                        }
                    }
                    Ok(Err(err)) => eprintln!("Reducer failed: {err}"),
                    Err(err) => eprintln!("Internal error: {err:?}"),
                }
            })?;
        }

        batch_done_rx.recv_timeout(Duration::from_secs(60))?;

        let stats = {
            let timer = roundtrip_timer.lock().unwrap();
            timer.stats()
        };

        all_stats.push((total_messages, stats));
    }

    // Summary
    println!("=== SUMMARY ===");
    println!(
        "{:>15} {:>12} {:>12} {:>12}",
        "Total messages", "Avg (ms)", "P50 (ms)", "P99 (ms)"
    );
    for (total, stats) in &all_stats {
        println!(
            "{:>10} {:>12.2} {:>12.2} {:>12.2}",
            total,
            stats.avg.as_secs_f64() * 1000.0,
            stats.p50.as_secs_f64() * 1000.0,
            stats.p99.as_secs_f64() * 1000.0,
        );
    }

    Ok(())
}
