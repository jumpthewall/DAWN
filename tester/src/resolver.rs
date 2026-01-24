use anyhow::{anyhow, Result};
use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::{Name, RData, RecordType};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::TokioAsyncResolver;
use rand::Rng;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

use crate::censorship::ForgedIps;
use crate::wasm_worker::WasmWorker;

const DNS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DNS_PACKET_SIZE: usize = 512;

/// Result of a single DNS resolution attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionResult {
    /// Response contained a forged/censored IP
    Censored,
    /// Response was valid and not censored
    NotCensored,
    /// Request timed out
    Timeout,
    /// An error occurred during resolution
    Error,
}

/// Aggregated test results for a resolver strategy
#[derive(Debug, Clone, Default)]
pub struct TestResults {
    pub total: usize,
    pub censored: usize,
    pub not_censored: usize,
    pub timeouts: usize,
    pub errors: usize,
}

impl TestResults {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, result: ResolutionResult) {
        self.total += 1;
        match result {
            ResolutionResult::Censored => self.censored += 1,
            ResolutionResult::NotCensored => self.not_censored += 1,
            ResolutionResult::Timeout => self.timeouts += 1,
            ResolutionResult::Error => self.errors += 1,
        }
    }

    pub fn censored_percentage(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.censored as f64 / self.total as f64) * 100.0
    }

    pub fn not_censored_percentage(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.not_censored as f64 / self.total as f64) * 100.0
    }
}

/// Test domain resolution using the system resolver
pub async fn test_system_resolver(
    domain: &str,
    forged_ips: &ForgedIps,
) -> ResolutionResult {
    let resolver = TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

    match timeout(DNS_TIMEOUT, resolver.lookup_ip(domain)).await {
        Ok(Ok(lookup)) => {
            let ips: Vec<IpAddr> = lookup.iter().map(IpAddr::from).collect();
            if ips.is_empty() {
                ResolutionResult::NotCensored
            } else if forged_ips.any_forged(&ips) {
                ResolutionResult::Censored
            } else {
                ResolutionResult::NotCensored
            }
        }
        Ok(Err(_)) => ResolutionResult::Error,
        Err(_) => ResolutionResult::Timeout,
    }
}

/// Test domain resolution using a WASM plugin strategy
pub async fn test_plugin_resolver(
    domain: &str,
    forged_ips: &ForgedIps,
    worker: &Arc<WasmWorker>,
    upstream: &SocketAddr,
) -> ResolutionResult {
    // Build DNS query
    let query = match build_dns_query(domain) {
        Ok(q) => q,
        Err(_) => return ResolutionResult::Error,
    };

    // Transform with WASM plugin
    let transformed = match worker.transform(&query).await {
        Ok(t) => t,
        Err(_) => return ResolutionResult::Error,
    };

    // Send to upstream and get response
    match send_dns_query(&transformed, upstream).await {
        Ok(response) => {
            let ips = extract_ips_from_response(&response);
            if ips.is_empty() {
                ResolutionResult::NotCensored
            } else if forged_ips.any_forged(&ips) {
                ResolutionResult::Censored
            } else {
                ResolutionResult::NotCensored
            }
        }
        Err(e) => {
            if e.to_string().contains("timeout") {
                ResolutionResult::Timeout
            } else {
                ResolutionResult::Error
            }
        }
    }
}

/// Build a DNS A query for a domain
fn build_dns_query(domain: &str) -> Result<Vec<u8>> {
    let name = Name::from_ascii(domain)
        .map_err(|e| anyhow!("Invalid domain name: {}", e))?;

    let mut message = Message::new();
    message.set_id(rand::thread_rng().gen());
    message.set_message_type(MessageType::Query);
    message.set_op_code(OpCode::Query);
    message.set_recursion_desired(true);

    let query = Query::query(name, RecordType::A);
    message.add_query(query);

    message
        .to_bytes()
        .map_err(|e| anyhow!("Failed to encode DNS query: {}", e))
}

/// Send DNS query via UDP and wait for response
async fn send_dns_query(query: &[u8], upstream: &SocketAddr) -> Result<Vec<u8>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(upstream).await?;

    socket.send(query).await?;

    let mut buf = [0u8; MAX_DNS_PACKET_SIZE];
    let len = timeout(DNS_TIMEOUT, socket.recv(&mut buf))
        .await
        .map_err(|_| anyhow!("DNS query timeout"))?
        .map_err(|e| anyhow!("Failed to receive DNS response: {}", e))?;

    Ok(buf[..len].to_vec())
}

/// Extract IP addresses from a DNS response
fn extract_ips_from_response(response: &[u8]) -> Vec<IpAddr> {
    let mut ips = Vec::new();

    if let Ok(message) = Message::from_bytes(response) {
        for answer in message.answers() {
            match answer.data() {
                Some(RData::A(a)) => ips.push(IpAddr::V4(a.0)),
                Some(RData::AAAA(aaaa)) => ips.push(IpAddr::V6(aaaa.0)),
                _ => {}
            }
        }
    }

    ips
}
