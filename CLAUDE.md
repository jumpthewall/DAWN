# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

DAWN (DNS Anti-censorship WebAssembly Nexus) is a DNS proxy that supports pluggable WASM modules for packet transformation. The goal is to democratize anti-censorship by letting anyone write efficient evasion techniques as portable WebAssembly plugins.

## Build Commands

```bash
# Build everything (proxy + plugins)
nix build .#bundle

# Build components separately
nix build .#proxy          # Static musl binary
nix build .#doublerPlugin  # WASM plugin

# Development
nix develop                # Enter dev shell
cargo test                 # Run all tests
cargo build -p dawn_doubler --target wasm32-unknown-unknown --release

# Run
./result/bin/dawn --plugin ./result/lib/dawn_doubler.wasm
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
dawn/
├── Cargo.toml              # Workspace root
├── flake.nix               # Nix build (musl proxy + wasm plugins)
├── proxy/
│   ├── Cargo.toml          # deps: wasmtime, tokio, clap, anyhow
│   └── src/
│       ├── main.rs         # CLI parsing, UDP server, task spawning
│       └── wasm_worker.rs  # Wasmtime host, channel handling
└── plugins/
    └── doubler/
        ├── Cargo.toml      # crate-type = ["cdylib"], wee_alloc
        └── src/
            └── lib.rs      # #![no_std] plugin implementation
```

## Key Files

### proxy/src/main.rs
- Parses CLI args with clap (`--plugin`, `--listen`, `--upstream`)
- Binds UDP socket, spawns task per incoming query
- Calls `WasmWorker::transform()` for packet transformation
- Forwards to upstream, returns response to client

### proxy/src/wasm_worker.rs
- `WasmWorker::new()` spawns a std::thread with Wasmtime
- Loads WASM module, extracts `alloc`/`dealloc`/`transform` exports
- `transform()` method sends request via mpsc, awaits oneshot response
- Worker thread: allocates WASM memory, copies data, calls transform, reads result

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
