# DAWN

**DNS Anti-censorship WebAssembly Nexus**

DAWN is a DNS proxy that democratizes anti-censorship by allowing anyone to write pluggable, efficient WebAssembly modules for DNS packet transformation. Write your evasion technique once in any language that compiles to WASM, and deploy it anywhere.

## Why DAWN?

Censorship evasion techniques often require custom protocol manipulation, but implementing them typically means:
- Forking existing tools and maintaining patches
- Writing in a specific language
- Dealing with platform-specific builds

DAWN solves this by separating the **transport** (a fast, async Rust proxy) from the **transformation logic** (portable WASM plugins). Anyone can write a plugin in Rust, C, Zig, or any language targeting WASM, and share it with the community.

## Quick Start

```bash
# Build everything
nix build .#bundle

# Run the proxy with a plugin
./result/bin/dawn --plugin ./result/lib/dawn_doubler.wasm

# Test it
dig @127.0.0.1 -p 1053 example.com A
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        DAWN Proxy                           │
│                                                             │
│   UDP Socket ──→ Tokio Tasks ──→ WASM Worker Thread         │
│        ↑              │                   │                 │
│        │              ↓                   ↓                 │
│        │         mpsc channel      Wasmtime Runtime         │
│        │              │                   │                 │
│        │              ↓                   │                 │
│        └──── oneshot response ←───────────┘                 │
│                                                             │
│   Client → Transform Plugin → Upstream DNS → Response       │
└─────────────────────────────────────────────────────────────┘
```

The proxy runs a dedicated thread with Wasmtime for plugin execution, communicating via channels. This keeps the async UDP handling separate from the synchronous WASM runtime.

## Writing Plugins

Plugins are WASM modules exporting three functions:

```rust
// Allocate memory for the host to write into
extern "C" fn alloc(size: u32) -> *mut u8;

// Free previously allocated memory
extern "C" fn dealloc(ptr: *mut u8, size: u32);

// Transform a DNS packet, returns output length
extern "C" fn transform(
    input_ptr: *const u8,
    input_len: u32,
    output_ptr: *mut u8,
    output_capacity: u32,
) -> u32;
```

See `plugins/doubler/` for a complete example that duplicates DNS questions using compression pointers.

### Plugin Development Tips

- Use `#![no_std]` with `wee_alloc` for minimal binary size
- The transform function receives raw DNS wire format
- Return the number of bytes written to the output buffer
- If transform fails, copy input to output unchanged

## Building

Requires [Nix](https://nixos.org/) with flakes enabled.

```bash
# Build the complete bundle (proxy + plugins)
nix build .#bundle

# Build components separately
nix build .#proxy          # Static musl binary
nix build .#doublerPlugin  # WASM plugin

# Development shell with full toolchain
nix develop
cargo test
cargo build -p dawn_doubler --target wasm32-unknown-unknown --release
```

## CLI Reference

```
dawn --plugin <path> [--listen <addr>] [--upstream <addr>]

Options:
  --plugin <path>      Path to WASM plugin module (required)
  --listen <addr>      Listen address [default: 127.0.0.1:1053]
  --upstream <addr>    Upstream DNS server [default: 8.8.8.8:53]
```

## Included Plugins

### doubler

Duplicates A, AAAA, and CNAME questions in DNS queries using compression pointers. This is a proof-of-concept demonstrating the plugin architecture.

## License

[TODO: Add license]

## Contributing

Contributions welcome! Whether it's new evasion plugins, protocol support, or documentation improvements.
