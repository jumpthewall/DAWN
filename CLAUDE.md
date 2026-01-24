# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

This project uses Nix flakes for building. The default build produces a statically-linked musl binary.

```bash
# Build (produces static musl binary)
nix build .#

# Enter dev shell with full toolchain (rust-analyzer, clippy, rustfmt)
nix develop

# Run tests
nix develop -c cargo test

# Run the server
./result/bin/dns_doubler
```

## Architecture

DNS Doubler is a DNS proxy that duplicates questions in DNS queries using compression pointers before forwarding upstream.

**Flow:** Client → localhost:1053 → Parse/Duplicate → 8.8.8.8:53 → Response → Client

**Files:**
- `src/main.rs` - Async UDP server using tokio. Binds to 127.0.0.1:1053, spawns task per query, forwards to upstream DNS.
- `src/doubler.rs` - Core logic for question duplication. Uses hickory-proto for parsing, then manually constructs wire-format packets with DNS compression pointers (0xC000 | offset).

**Key constants:** `LISTEN_ADDR` (127.0.0.1:1053), `UPSTREAM_DNS` (8.8.8.8:53)

**Duplication logic:** Only A, AAAA, and CNAME queries are duplicated. Duplicates use compression pointers to reference original question names, adding 6 bytes per duplicate (2-byte pointer + 2-byte QTYPE + 2-byte QCLASS).
