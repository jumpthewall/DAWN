# DAWN Plugin Development Guide

This guide explains how to write WebAssembly plugins for DAWN. Plugins transform DNS packets before they're sent upstream, enabling various censorship evasion techniques.

## Overview

DAWN plugins are WebAssembly modules that export a specific ABI. The proxy loads your plugin, passes DNS packets through your `transform` function, and forwards the result to the upstream DNS server.

```
Client Query → DAWN Proxy → Your Plugin → Upstream DNS
                              ↓
                     transform(packet) → modified packet
```

## Quick Start

1. Create a new Rust library:

```bash
cargo new --lib my_plugin
cd my_plugin
```

2. Configure `Cargo.toml`:

```toml
[package]
name = "dawn_myplugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]  # cdylib for WASM, rlib for tests

[dependencies]
dawn_plugin_common = { git = "https://github.com/jumpthewall/DAWN", path = "plugins/common" }

[profile.release]
opt-level = "s"    # Optimize for size
lto = true         # Link-time optimization
panic = "abort"    # No unwinding in WASM
```

3. Implement the plugin in `src/lib.rs`:

```rust
use dawn_plugin_common::slice;

// Re-export alloc/dealloc from the common crate
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    dawn_plugin_common::plugin_alloc(size)
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, size: u32) {
    dawn_plugin_common::plugin_dealloc(ptr, size)
}

#[no_mangle]
pub unsafe extern "C" fn transform(
    input_ptr: *const u8,
    input_len: u32,
    output_ptr: *mut u8,
    output_capacity: u32,
) -> u32 {
    let input = slice::from_raw_parts(input_ptr, input_len as usize);
    let output = slice::from_raw_parts_mut(output_ptr, output_capacity as usize);

    // Your transformation logic here
    // This example just copies the packet unchanged
    let len = core::cmp::min(input.len(), output.len());
    output[..len].copy_from_slice(&input[..len]);
    len as u32
}
```

The `dawn_plugin_common` crate provides:
- `plugin_alloc` / `plugin_dealloc` - Memory management for WASM
- `slice` - Re-exported `core::slice` for pointer conversions
- `Vec` - Re-exported `std::vec::Vec`

4. Build the plugin:

```bash
cargo build --target wasm32-unknown-unknown --release
```

The compiled plugin will be at `target/wasm32-unknown-unknown/release/dawn_myplugin.wasm`.

5. Test with DAWN:

```bash
dawn --plugin ./target/wasm32-unknown-unknown/release/dawn_myplugin.wasm
```

## Plugin ABI Reference

Your plugin must export these three functions:

### `alloc(size: u32) -> *mut u8`

Allocates `size` bytes in the WASM linear memory and returns a pointer. The host uses this to create buffers for input and output data.

### `dealloc(ptr: *mut u8, size: u32)`

Frees memory previously allocated by `alloc`. Called after the transform completes.

### `transform(input_ptr, input_len, output_ptr, output_capacity) -> u32`

Transforms a DNS packet. Parameters:
- `input_ptr`: Pointer to the input DNS packet (read-only)
- `input_len`: Length of the input packet in bytes
- `output_ptr`: Pointer to the output buffer (write your result here)
- `output_capacity`: Maximum bytes you can write to output

Returns the number of bytes written to the output buffer.

### Memory Protocol

The host follows this sequence:

```
1. input_ptr  = plugin.alloc(input_len)
2. output_ptr = plugin.alloc(output_capacity)
3. host writes input data to input_ptr
4. output_len = plugin.transform(input_ptr, input_len, output_ptr, output_capacity)
5. host reads output_len bytes from output_ptr
6. plugin.dealloc(input_ptr, input_len)
7. plugin.dealloc(output_ptr, output_capacity)
```

## DNS Packet Format

DNS packets arrive in wire format (RFC 1035). Here's the header structure:

```
 0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                      ID                       |  Bytes 0-1
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|QR|   OPCODE  |AA|TC|RD|RA|   Z    |   RCODE   |  Bytes 2-3
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                    QDCOUNT                    |  Bytes 4-5
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                    ANCOUNT                    |  Bytes 6-7
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                    NSCOUNT                    |  Bytes 8-9
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|                    ARCOUNT                    |  Bytes 10-11
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
```

After the 12-byte header come the question, answer, authority, and additional sections.

### Domain Name Encoding

Domain names use length-prefixed labels:
- `example.com` → `\x07example\x03com\x00`
- Each label starts with its length byte
- Names end with a zero byte

Compression pointers (2 bytes starting with `0xC0`) can reference earlier names in the packet.

## Best Practices

### Error Handling

If your transform fails, copy the input to output unchanged:

```rust
fn transform(...) -> u32 {
    match my_transform(input, output) {
        Some(len) => len as u32,
        None => {
            // Fallback: copy unchanged
            let len = core::cmp::min(input.len(), output.len());
            output[..len].copy_from_slice(&input[..len]);
            len as u32
        }
    }
}
```

### Testing

Use `#![cfg_attr(...)]` to support both WASM and native targets:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_transform() {
        let input = [...];  // Raw DNS packet bytes
        let mut output = [0u8; 512];
        let len = my_transform(&input, &mut output);
        assert!(len > 0);
        // Verify the output...
    }
}
```

Run tests with:
```bash
cargo test  # Runs on native target, not WASM
```

### Binary Size

Keep plugins small for fast loading:

- Enable LTO and size optimization in release profile
- Avoid pulling in heavy dependencies

Typical plugin size: 10-30 KB.

### Debugging

For development, you can print debug info (won't work in WASM):

```rust
#[cfg(not(target_arch = "wasm32"))]
eprintln!("Debug: packet len = {}", input.len());
```

## Example Plugins

### doubler

Duplicates A/AAAA/CNAME questions using DNS compression pointers. This can confuse some censorship systems that only inspect the first question.

See: `plugins/doubler/src/lib.rs`

### iquery

Sets the IQUERY (inverse query) opcode in DNS headers. Some censors don't inspect packets with unusual opcodes.

See: `plugins/iquery/src/lib.rs`

## Building with Nix

If using the project's Nix flake:

```bash
# Build a specific plugin
nix build .#doublerPlugin

# The WASM file will be at result/lib/dawn_doubler.wasm
```

To add your plugin to the Nix build, add it to `flake.nix`:

```nix
myPlugin = buildPlugin ./plugins/myplugin;
```

## Useful Resources

- [RFC 1035](https://tools.ietf.org/html/rfc1035) - DNS protocol specification
- [DNS packet format](https://datatracker.ietf.org/doc/html/rfc1035#section-4) - Wire format details
- [Wasmtime](https://wasmtime.dev/) - The WASM runtime DAWN uses
