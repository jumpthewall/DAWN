mod censorship;
mod report;
mod resolver;
mod wasm_worker;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;

use censorship::{load_domains, ForgedIps};
use report::{clear_progress, print_progress, print_report};
use resolver::{test_plugin_resolver, test_system_resolver, TestResults};
use wasm_worker::WasmWorker;

#[derive(Parser, Debug)]
#[command(name = "dawn-tester")]
#[command(about = "Test DNS censorship detection and evasion effectiveness")]
struct Args {
    /// Comma-separated WASM plugin paths
    #[arg(long)]
    plugins: Option<String>,

    /// Domain list file
    #[arg(long, default_value = "data/censored.txt")]
    domains: PathBuf,

    /// Forged IPv4 addresses file
    #[arg(long, default_value = "data/forged.ipv4")]
    forged_ipv4: PathBuf,

    /// Forged IPv6 addresses file
    #[arg(long, default_value = "data/forged.ipv6")]
    forged_ipv6: PathBuf,

    /// Upstream DNS server
    #[arg(long, default_value = "8.8.8.8:53")]
    upstream: String,

    /// Number of concurrent requests
    #[arg(long, default_value = "10")]
    concurrency: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("{}", "DAWN Censorship Tester".bold().cyan());
    println!();

    // Load forged IPs
    println!("Loading forged IP addresses...");
    let forged_ips = ForgedIps::load(&args.forged_ipv4, &args.forged_ipv6)
        .context("Failed to load forged IP addresses")?;
    println!(
        "  Loaded {} forged IPs ({} IPv4, {} IPv6)",
        forged_ips.total_count(),
        forged_ips.ipv4.len(),
        forged_ips.ipv6.len()
    );

    // Load domains
    println!("Loading domains...");
    let domains = load_domains(&args.domains)?;
    println!("  Loaded {} domains", domains.len());
    println!();

    // Parse upstream address
    let upstream: SocketAddr = args
        .upstream
        .parse()
        .context("Invalid upstream address")?;

    // Parse plugin paths
    let plugin_paths: Vec<&str> = args
        .plugins
        .as_deref()
        .map(|p| p.split(',').map(|s| s.trim()).collect())
        .unwrap_or_default();

    // Test system resolver
    println!("{}", "Testing system resolver...".yellow());
    let system_results = test_domains_system(&domains, &forged_ips, args.concurrency).await;
    clear_progress();
    println!("  Completed: {} domains tested", system_results.total);

    // Test each plugin
    let mut plugin_results: Vec<(String, TestResults)> = Vec::new();
    for plugin_path in &plugin_paths {
        println!();
        println!("{} {}...", "Testing plugin:".yellow(), plugin_path);

        let worker = WasmWorker::new(plugin_path)
            .with_context(|| format!("Failed to load plugin: {}", plugin_path))?;
        let worker = Arc::new(worker);

        let results =
            test_domains_plugin(&domains, &forged_ips, &worker, &upstream, args.concurrency)
                .await;
        clear_progress();
        println!("  Completed: {} domains tested", results.total);

        // Extract plugin name from path for display
        let plugin_name = PathBuf::from(plugin_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| plugin_path.to_string());

        plugin_results.push((plugin_name, results));
    }

    // Convert plugin_results for report
    let report_results: Vec<(&str, TestResults)> = plugin_results
        .iter()
        .map(|(name, results)| (name.as_ref(), results.clone()))
        .collect();

    // Print report
    print_report(&system_results, &report_results);

    Ok(())
}

/// Test all domains using the system resolver
async fn test_domains_system(
    domains: &[String],
    forged_ips: &ForgedIps,
    concurrency: usize,
) -> TestResults {
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let results = Arc::new(tokio::sync::Mutex::new(TestResults::new()));
    let total = domains.len();

    let mut handles = Vec::new();

    for domain in domains {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let domain = domain.clone();
        let forged_ips_v4 = forged_ips.ipv4.clone();
        let forged_ips_v6 = forged_ips.ipv6.clone();
        let results = Arc::clone(&results);

        let handle = tokio::spawn(async move {
            let forged = ForgedIps {
                ipv4: forged_ips_v4,
                ipv6: forged_ips_v6,
            };
            let result = test_system_resolver(&domain, &forged).await;

            let mut results = results.lock().await;
            results.record(result);
            let completed = results.total;
            drop(results);

            print_progress("System", completed, total);

            drop(permit);
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    let results = results.lock().await;
    results.clone()
}

/// Test all domains using a WASM plugin
async fn test_domains_plugin(
    domains: &[String],
    forged_ips: &ForgedIps,
    worker: &Arc<WasmWorker>,
    upstream: &SocketAddr,
    concurrency: usize,
) -> TestResults {
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let results = Arc::new(tokio::sync::Mutex::new(TestResults::new()));
    let total = domains.len();

    let mut handles = Vec::new();

    for domain in domains {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let domain = domain.clone();
        let forged_ips_v4 = forged_ips.ipv4.clone();
        let forged_ips_v6 = forged_ips.ipv6.clone();
        let worker = Arc::clone(worker);
        let upstream = *upstream;
        let results = Arc::clone(&results);

        let handle = tokio::spawn(async move {
            let forged = ForgedIps {
                ipv4: forged_ips_v4,
                ipv6: forged_ips_v6,
            };
            let result = test_plugin_resolver(&domain, &forged, &worker, &upstream).await;

            let mut results = results.lock().await;
            results.record(result);
            let completed = results.total;
            drop(results);

            print_progress("Plugin", completed, total);

            drop(permit);
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    let results = results.lock().await;
    results.clone()
}
