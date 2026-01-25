#![cfg_attr(target_arch = "wasm32", no_std)]

#[cfg(target_arch = "wasm32")]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
use alloc::vec::Vec;

#[cfg(not(target_arch = "wasm32"))]
use std::vec::Vec;

use core::slice;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

/// DNS header size in bytes
const HEADER_SIZE: usize = 12;

/// Mask to clear opcode bits (bits 1-4 of byte 2)
/// Binary: 10000111 = 0x87
const OPCODE_CLEAR_MASK: u8 = 0x87;

/// IQUERY opcode value (1) shifted into position (bits 1-4)
/// Binary: 00001000 = 0x08
const IQUERY_OPCODE: u8 = 0x08;

/// Allocate memory in WASM linear memory
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    let mut buf = Vec::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

/// Deallocate memory in WASM linear memory
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, size: u32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size as usize);
    }
}

/// Transform a DNS packet by setting the IQUERY opcode
///
/// Modifies byte 2 of the DNS header to set opcode to IQUERY (1)
/// while preserving the RD (Recursion Desired) bit.
///
/// Returns the number of bytes written to the output buffer
#[no_mangle]
pub extern "C" fn transform(
    input_ptr: *const u8,
    input_len: u32,
    output_ptr: *mut u8,
    output_capacity: u32,
) -> u32 {
    let input = unsafe { slice::from_raw_parts(input_ptr, input_len as usize) };
    let output = unsafe { slice::from_raw_parts_mut(output_ptr, output_capacity as usize) };

    set_iquery_opcode(input, output) as u32
}

/// Sets the IQUERY opcode in a DNS packet while preserving RD bit.
///
/// DNS Header flags (bytes 2-3):
/// Byte 2: QR(1) OPCODE(4) AA(1) TC(1) RD(1)
/// Byte 3: RA(1) Z(3) RCODE(4)
///
/// IQUERY = opcode 1 = bits 1-4 of byte 2 set to 0001
fn set_iquery_opcode(input: &[u8], output: &mut [u8]) -> usize {
    // Ensure we have at least a DNS header
    if input.len() < HEADER_SIZE {
        // Packet too small, copy unchanged
        let copy_len = core::cmp::min(input.len(), output.len());
        output[..copy_len].copy_from_slice(&input[..copy_len]);
        return copy_len;
    }

    // Ensure output can hold the input
    let copy_len = core::cmp::min(input.len(), output.len());
    output[..copy_len].copy_from_slice(&input[..copy_len]);

    // Modify byte 2 (flags byte 1):
    // - Clear opcode bits (bits 1-4): AND with 0x87
    // - Set IQUERY opcode (1): OR with 0x08
    // - RD bit (bit 7) is preserved automatically
    output[2] = (output[2] & OPCODE_CLEAR_MASK) | IQUERY_OPCODE;

    copy_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_iquery_opcode_standard_query() {
        // Standard query with RD=1
        // Flags byte 2: 0x01 = 00000001 (QR=0, OPCODE=0, AA=0, TC=0, RD=1)
        let query = [
            0x12, 0x34, // ID
            0x01, 0x00, // Flags: standard query, RD=1
            0x00, 0x01, // QDCOUNT = 1
            0x00, 0x00, // ANCOUNT = 0
            0x00, 0x00, // NSCOUNT = 0
            0x00, 0x00, // ARCOUNT = 0
            // Question: test.com A IN
            4, b't', b'e', b's', b't', 3, b'c', b'o', b'm', 0, 0x00, 0x01, // QTYPE = A
            0x00, 0x01, // QCLASS = IN
        ];

        let mut output = [0u8; 256];
        let len = set_iquery_opcode(&query, &mut output);

        assert_eq!(len, query.len());

        // Check that IQUERY opcode is set
        // Expected: 0x09 = 00001001 (QR=0, OPCODE=1, AA=0, TC=0, RD=1)
        assert_eq!(output[2], 0x09);

        // Rest of packet should be unchanged
        assert_eq!(output[0], 0x12); // ID
        assert_eq!(output[1], 0x34);
        assert_eq!(output[3], 0x00); // Flags byte 2 unchanged
        assert_eq!(&output[4..len], &query[4..]);
    }

    #[test]
    fn test_set_iquery_opcode_no_rd() {
        // Query without RD bit
        // Flags byte 2: 0x00 = 00000000 (QR=0, OPCODE=0, AA=0, TC=0, RD=0)
        let query = [
            0x12, 0x34, // ID
            0x00, 0x00, // Flags: standard query, RD=0
            0x00, 0x01, // QDCOUNT = 1
            0x00, 0x00, // ANCOUNT = 0
            0x00, 0x00, // NSCOUNT = 0
            0x00, 0x00, // ARCOUNT = 0
        ];

        let mut output = [0u8; 256];
        let len = set_iquery_opcode(&query, &mut output);

        assert_eq!(len, query.len());

        // Expected: 0x08 = 00001000 (QR=0, OPCODE=1, AA=0, TC=0, RD=0)
        assert_eq!(output[2], 0x08);
    }

    #[test]
    fn test_packet_too_small() {
        // Packet smaller than DNS header
        let query = [0x12, 0x34, 0x01];

        let mut output = [0u8; 256];
        let len = set_iquery_opcode(&query, &mut output);

        // Should copy unchanged
        assert_eq!(len, query.len());
        assert_eq!(&output[..len], &query[..]);
    }
}
