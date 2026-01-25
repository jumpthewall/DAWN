//! Censorship detection via forged IP address matching.
//!
//! Provides utilities for loading and checking against known forged IP addresses
//! used by DNS censorship systems.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::Path;

/// Collection of known forged/censorship IP addresses.
pub struct ForgedIps {
    pub ipv4: HashSet<IpAddr>,
    pub ipv6: HashSet<IpAddr>,
}

impl ForgedIps {
    /// Create a new empty ForgedIps collection
    pub fn new() -> Self {
        Self {
            ipv4: HashSet::new(),
            ipv6: HashSet::new(),
        }
    }

    /// Load forged IPs from files
    pub fn load(ipv4_path: &Path, ipv6_path: &Path) -> Result<Self> {
        let ipv4 = load_ip_file(ipv4_path)
            .with_context(|| format!("Failed to load IPv4 forged IPs from {:?}", ipv4_path))?;
        let ipv6 = load_ip_file(ipv6_path)
            .with_context(|| format!("Failed to load IPv6 forged IPs from {:?}", ipv6_path))?;

        Ok(Self { ipv4, ipv6 })
    }

    /// Check if an IP address is in the forged list
    pub fn is_forged(&self, ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(_) => self.ipv4.contains(ip),
            IpAddr::V6(_) => self.ipv6.contains(ip),
        }
    }

    /// Check if any of the given IP addresses are forged
    pub fn any_forged(&self, ips: &[IpAddr]) -> bool {
        ips.iter().any(|ip| self.is_forged(ip))
    }

    /// Total count of forged IPs
    pub fn total_count(&self) -> usize {
        self.ipv4.len() + self.ipv6.len()
    }
}

impl Default for ForgedIps {
    fn default() -> Self {
        Self::new()
    }
}

/// Load IP addresses from a file (one per line)
fn load_ip_file(path: &Path) -> Result<HashSet<IpAddr>> {
    let content = fs::read_to_string(path)?;
    let mut ips = HashSet::new();

    for line in content.lines() {
        let line = line.trim();
        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Ok(ip) = line.parse::<IpAddr>() {
            ips.insert(ip);
        }
    }

    Ok(ips)
}

/// Load domains from a file (one per line)
pub fn load_domains(path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read domains file {:?}", path))?;

    let domains: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect();

    Ok(domains)
}
