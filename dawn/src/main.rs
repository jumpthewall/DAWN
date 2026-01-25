mod wasm_worker;

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use wasm_worker::WasmWorker;

const MAX_DNS_PACKET_SIZE: usize = 512;

#[derive(Parser, Debug)]
#[command(name = "dawn")]
#[command(
    about = "DNS Anti-censorship WebAssembly Nexus - A DNS proxy with pluggable WASM transforms"
)]
struct Args {
    /// Path to the WASM plugin module
    #[arg(long)]
    plugin: String,

    /// Address to listen on
    #[arg(long, default_value = "127.0.0.1:1053")]
    listen: String,

    /// Upstream DNS server
    #[arg(long, default_value = "8.8.8.8:53")]
    upstream: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("DAWN starting...");
    println!("  Plugin: {}", args.plugin);
    println!("  Listen: {}", args.listen);
    println!("  Upstream: {}", args.upstream);

    // Spawn WASM worker
    let worker = WasmWorker::new(&args.plugin)?;
    let worker = Arc::new(worker);

    let socket = UdpSocket::bind(&args.listen).await?;
    let socket = Arc::new(socket);

    println!("Ready to receive queries");

    let mut buf = [0u8; MAX_DNS_PACKET_SIZE];

    loop {
        let (len, client_addr) = socket.recv_from(&mut buf).await?;
        let query = buf[..len].to_vec();
        let socket_clone = Arc::clone(&socket);
        let worker_clone = Arc::clone(&worker);
        let upstream = args.upstream.clone();

        tokio::spawn(async move {
            if let Err(e) =
                handle_query(socket_clone, worker_clone, query, client_addr, &upstream).await
            {
                eprintln!("Error handling query from {}: {}", client_addr, e);
            }
        });
    }
}

async fn handle_query(
    socket: Arc<UdpSocket>,
    worker: Arc<WasmWorker>,
    query: Vec<u8>,
    client_addr: SocketAddr,
    upstream: &str,
) -> Result<()> {
    // Transform the query using the WASM plugin
    let modified_query = match worker.transform(&query).await {
        Ok(q) => {
            if q.len() != query.len() {
                println!(
                    "Query from {}: transformed ({} -> {} bytes)",
                    client_addr,
                    query.len(),
                    q.len()
                );
            }
            q
        }
        Err(e) => {
            eprintln!("Failed to transform query: {}, forwarding original", e);
            query
        }
    };

    // Create a new socket for upstream communication
    let upstream_socket = UdpSocket::bind("0.0.0.0:0").await?;
    upstream_socket.connect(upstream).await?;

    // Forward the (possibly modified) query upstream
    upstream_socket.send(&modified_query).await?;

    // Receive response from upstream
    let mut response_buf = [0u8; MAX_DNS_PACKET_SIZE];
    let response_len = upstream_socket.recv(&mut response_buf).await?;

    // Send response back to client
    socket
        .send_to(&response_buf[..response_len], client_addr)
        .await?;

    Ok(())
}
