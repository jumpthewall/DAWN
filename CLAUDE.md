# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

DAWN (DNS Anti-censorship WebAssembly Nexus) is a DNS proxy that supports pluggable WASM modules for packet transformation. The goal is to democratize anti-censorship by letting anyone write efficient evasion techniques as portable WebAssembly plugins.

## Build Commands

```bash
# Build everything (binaries + plugins)
nix build .#fullBundle

# Build components separately
nix build .#dawn           # Static musl binary (proxy + tester)
nix build .#doublerPlugin  # WASM plugin

# Development
nix develop                # Enter dev shell
cargo test                 # Run all tests
cargo build -p dawn_doubler --target wasm32-unknown-unknown --release

# Run
./result/bin/dawn --plugin ./result/lib/dawn_doubler.wasm
./result/bin/dawn-tester --plugins ./result/lib/dawn_doubler.wasm
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        DAWN Proxy                           │
│   UDP Socket ──→ Tokio Tasks ──→ WASM Worker Thread         │
│        ↑              │                   │                 │
│        └──── oneshot ←── mpsc ←── Wasmtime Runtime          │
└─────────────────────────────────────────────────────────────┘
```

**Flow:** Client → dawn proxy → WASM transform → Upstream DNS → Response → Client

The proxy uses a dedicated std::thread for Wasmtime to avoid blocking the Tokio runtime. Communication happens via mpsc (requests) and oneshot (responses) channels.

## Directory Structure

```
dns_doubler/
├── Cargo.toml              # Workspace root
├── flake.nix               # Nix build (musl binaries + wasm plugins)
├── dawn/
│   ├── Cargo.toml          # deps: wasmtime, tokio, clap, anyhow, hickory-*, colored, rand
│   └── src/
│       ├── main.rs         # CLI parsing, UDP server, task spawning
│       ├── lib.rs          # Library exports (wasm_worker, tester)
│       ├── wasm_worker.rs  # Wasmtime host, InstancePre-based parallelism
│       ├── bin/
│       │   └── dawn_tester.rs  # Tester binary entry point
│       └── tester/
│           ├── mod.rs      # Tester module exports
│           ├── censorship.rs   # Forged IP management
│           ├── resolver.rs     # DNS resolution testing
│           └── report.rs       # Test reporting
└── plugins/
    ├── doubler/
    │   ├── Cargo.toml      # crate-type = ["cdylib"], wee_alloc
    │   └── src/lib.rs      # #![no_std] doubler plugin
    └── iquery/
        ├── Cargo.toml      # crate-type = ["cdylib"], wee_alloc
        └── src/lib.rs      # #![no_std] iquery plugin
```

## Key Files

### dawn/src/main.rs
- Parses CLI args with clap (`--plugin`, `--listen`, `--upstream`)
- Binds UDP socket, spawns task per incoming query
- Calls `WasmWorker::transform()` for packet transformation
- Forwards to upstream, returns response to client

### dawn/src/wasm_worker.rs
- `WasmWorker::new()` loads WASM module with Wasmtime InstancePre
- Exports `alloc`/`dealloc`/`transform` functions
- `transform()` uses spawn_blocking for parallel WASM execution
- Shared between proxy and tester binaries

### dawn/src/bin/dawn_tester.rs
- Tests DNS censorship detection and evasion effectiveness
- Compares system resolver against WASM plugin strategies
- Uses `dawn::tester` and `dawn::wasm_worker` modules

### plugins/doubler/src/lib.rs
- `#![no_std]` with `wee_alloc` for minimal binary size
- Exports `alloc`, `dealloc`, `transform` with C ABI
- Duplicates A/AAAA/CNAME questions using DNS compression pointers
- Pure byte manipulation, no external DNS parsing libraries

## WASM Plugin ABI

All plugins must export:

```rust
extern "C" fn alloc(size: u32) -> *mut u8;
extern "C" fn dealloc(ptr: *mut u8, size: u32);
extern "C" fn transform(
    input_ptr: *const u8,
    input_len: u32,
    output_ptr: *mut u8,
    output_capacity: u32
) -> u32;  // Returns bytes written to output
```

**Memory protocol:**
1. Host calls `alloc(input_len)` → gets input_ptr
2. Host calls `alloc(output_capacity)` → gets output_ptr
3. Host writes input data to input_ptr
4. Host calls `transform(input_ptr, input_len, output_ptr, output_capacity)`
5. Host reads output_len bytes from output_ptr
6. Host calls `dealloc` for both buffers

## Testing

```bash
# Unit tests
cargo test

# Manual integration test
./result/bin/dawn --plugin ./result/lib/dawn_doubler.wasm &
dig @127.0.0.1 -p 1053 example.com A
# Should see "transformed" message in proxy output
```
