mod doubler;

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

const LISTEN_ADDR: &str = "127.0.0.1:1053";
const UPSTREAM_DNS: &str = "8.8.8.8:53";
const MAX_DNS_PACKET_SIZE: usize = 512;

#[tokio::main]
async fn main() -> Result<()> {
    println!("DNS Doubler starting...");
    println!("Listening on: {}", LISTEN_ADDR);
    println!("Upstream DNS: {}", UPSTREAM_DNS);

    let socket = UdpSocket::bind(LISTEN_ADDR).await?;
    let socket = Arc::new(socket);

    println!("Ready to receive queries");

    let mut buf = [0u8; MAX_DNS_PACKET_SIZE];

    loop {
        let (len, client_addr) = socket.recv_from(&mut buf).await?;
        let query = buf[..len].to_vec();
        let socket_clone = Arc::clone(&socket);

        tokio::spawn(async move {
            if let Err(e) = handle_query(socket_clone, query, client_addr).await {
                eprintln!("Error handling query from {}: {}", client_addr, e);
            }
        });
    }
}

async fn handle_query(
    socket: Arc<UdpSocket>,
    query: Vec<u8>,
    client_addr: SocketAddr,
) -> Result<()> {
    // Duplicate questions if this is an A/AAAA/CNAME query
    let modified_query = match doubler::duplicate_questions(&query) {
        Ok(q) => {
            if q.len() != query.len() {
                println!(
                    "Query from {}: duplicated questions ({} -> {} bytes)",
                    client_addr,
                    query.len(),
                    q.len()
                );
            }
            q
        }
        Err(e) => {
            eprintln!("Failed to process query: {}, forwarding original", e);
            query
        }
    };

    // Create a new socket for upstream communication
    let upstream_socket = UdpSocket::bind("0.0.0.0:0").await?;
    upstream_socket.connect(UPSTREAM_DNS).await?;

    // Forward the (possibly modified) query upstream
    upstream_socket.send(&modified_query).await?;

    // Receive response from upstream
    let mut response_buf = [0u8; MAX_DNS_PACKET_SIZE];
    let response_len = upstream_socket.recv(&mut response_buf).await?;

    // Send response back to client
    socket.send_to(&response_buf[..response_len], client_addr).await?;

    Ok(())
}
