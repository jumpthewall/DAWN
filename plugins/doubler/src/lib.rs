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

/// Record types we duplicate
const TYPE_A: u16 = 1;
const TYPE_CNAME: u16 = 5;
const TYPE_AAAA: u16 = 28;

/// Check if the record type should have its question duplicated
fn should_duplicate(qtype: u16) -> bool {
    qtype == TYPE_A || qtype == TYPE_AAAA || qtype == TYPE_CNAME
}

/// Allocate memory in WASM linear memory
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> *mut u8 {
    let mut buf = Vec::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

/// Deallocate memory in WASM linear memory
///
/// # Safety
/// `ptr` must have been allocated by `alloc` with the same `size`.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, size: u32) {
    // SAFETY: caller guarantees ptr was allocated by alloc with this size
    let _ = Vec::from_raw_parts(ptr, 0, size as usize);
}

/// Transform a DNS packet by duplicating questions
///
/// Returns the number of bytes written to the output buffer
///
/// # Safety
/// - `input_ptr` must be valid for reads of `input_len` bytes
/// - `output_ptr` must be valid for writes of `output_capacity` bytes
#[no_mangle]
pub unsafe extern "C" fn transform(
    input_ptr: *const u8,
    input_len: u32,
    output_ptr: *mut u8,
    output_capacity: u32,
) -> u32 {
    // SAFETY: caller guarantees pointers are valid for the given lengths
    let input = slice::from_raw_parts(input_ptr, input_len as usize);
    let output = slice::from_raw_parts_mut(output_ptr, output_capacity as usize);

    match duplicate_questions(input, output) {
        Some(len) => len as u32,
        None => {
            // On error, copy original packet
            let copy_len = core::cmp::min(input.len(), output.len());
            output[..copy_len].copy_from_slice(&input[..copy_len]);
            copy_len as u32
        }
    }
}

/// Duplicates questions in a DNS query using compression pointers.
///
/// Returns the number of bytes written to output, or None on error.
fn duplicate_questions(input: &[u8], output: &mut [u8]) -> Option<usize> {
    if input.len() < HEADER_SIZE {
        return None;
    }

    // Read QDCOUNT
    let qdcount = u16::from_be_bytes([input[4], input[5]]);
    if qdcount == 0 {
        // No questions to duplicate
        let len = input.len();
        if len > output.len() {
            return None;
        }
        output[..len].copy_from_slice(input);
        return Some(len);
    }

    // First pass: find question offsets and types
    let mut question_info: [(usize, u16, u16); 16] = [(0, 0, 0); 16]; // (offset, qtype, qclass)
    let mut offset = HEADER_SIZE;
    let mut need_duplication = false;

    let qdcount_usize = qdcount as usize;
    if qdcount_usize > 16 {
        return None; // Too many questions
    }

    for info in question_info.iter_mut().take(qdcount_usize) {
        let name_start = offset;

        // Skip the name
        offset = skip_name(input, offset)?;

        // Read QTYPE and QCLASS
        if offset + 4 > input.len() {
            return None;
        }
        let qtype = u16::from_be_bytes([input[offset], input[offset + 1]]);
        let qclass = u16::from_be_bytes([input[offset + 2], input[offset + 3]]);
        offset += 4;

        *info = (name_start, qtype, qclass);

        if should_duplicate(qtype) {
            need_duplication = true;
        }
    }

    if !need_duplication {
        // No duplication needed, copy original
        let len = input.len();
        if len > output.len() {
            return None;
        }
        output[..len].copy_from_slice(input);
        return Some(len);
    }

    // Build output packet
    let questions_end = offset;

    // Calculate output size needed
    let duplicates_size = (qdcount as usize) * 6; // 2-byte pointer + 2-byte type + 2-byte class
    let output_size = questions_end + duplicates_size + (input.len() - questions_end);

    if output_size > output.len() {
        return None;
    }

    // Copy header
    output[..HEADER_SIZE].copy_from_slice(&input[..HEADER_SIZE]);

    // Update QDCOUNT to double
    let new_qdcount = qdcount.saturating_mul(2);
    output[4] = (new_qdcount >> 8) as u8;
    output[5] = (new_qdcount & 0xFF) as u8;

    // Copy original questions
    output[HEADER_SIZE..questions_end].copy_from_slice(&input[HEADER_SIZE..questions_end]);

    // Add duplicates using compression pointers
    let mut out_offset = questions_end;
    for &(name_offset, qtype, qclass) in question_info.iter().take(qdcount_usize) {
        // Compression pointer: 0xC000 | offset
        let pointer = 0xC000 | (name_offset as u16);
        output[out_offset] = (pointer >> 8) as u8;
        output[out_offset + 1] = (pointer & 0xFF) as u8;

        // QTYPE
        output[out_offset + 2] = (qtype >> 8) as u8;
        output[out_offset + 3] = (qtype & 0xFF) as u8;

        // QCLASS
        output[out_offset + 4] = (qclass >> 8) as u8;
        output[out_offset + 5] = (qclass & 0xFF) as u8;

        out_offset += 6;
    }

    // Copy any remaining data (shouldn't be any for a query, but just in case)
    if questions_end < input.len() {
        let remaining = input.len() - questions_end;
        output[out_offset..out_offset + remaining].copy_from_slice(&input[questions_end..]);
        out_offset += remaining;
    }

    Some(out_offset)
}

/// Skip a DNS name in the wire format, handling labels and compression pointers.
/// Returns the offset after the name.
fn skip_name(data: &[u8], mut offset: usize) -> Option<usize> {
    loop {
        if offset >= data.len() {
            return None;
        }

        let len = data[offset];

        if len == 0 {
            // Null terminator
            return Some(offset + 1);
        } else if (len & 0xC0) == 0xC0 {
            // Compression pointer (2 bytes)
            return Some(offset + 2);
        } else if (len & 0xC0) == 0 {
            // Regular label
            offset += 1 + len as usize;
        } else {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_name_simple() {
        // "example.com" = 7example3com0
        let data = [
            7, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 3, b'c', b'o', b'm', 0,
        ];
        assert_eq!(skip_name(&data, 0), Some(13));
    }

    #[test]
    fn test_duplicate_a_query() {
        // Build a simple A query for "test.com"
        let query = [
            0x12, 0x34, // ID
            0x01, 0x00, // Flags (standard query)
            0x00, 0x01, // QDCOUNT = 1
            0x00, 0x00, // ANCOUNT = 0
            0x00, 0x00, // NSCOUNT = 0
            0x00, 0x00, // ARCOUNT = 0
            // Question: test.com A IN
            4, b't', b'e', b's', b't', 3, b'c', b'o', b'm', 0, 0x00, 0x01, // QTYPE = A
            0x00, 0x01, // QCLASS = IN
        ];

        let mut output = [0u8; 256];
        let len = duplicate_questions(&query, &mut output).unwrap();

        // Check QDCOUNT is now 2
        assert_eq!(output[4], 0x00);
        assert_eq!(output[5], 0x02);

        // Original question should be preserved
        assert_eq!(&output[12..26], &query[12..26]);

        // Duplicate should use compression pointer
        assert_eq!(output[26], 0xC0); // Compression pointer high byte
        assert_eq!(output[27], 0x0C); // Points to offset 12
        assert_eq!(output[28], 0x00); // QTYPE high byte
        assert_eq!(output[29], 0x01); // QTYPE = A
        assert_eq!(output[30], 0x00); // QCLASS high byte
        assert_eq!(output[31], 0x01); // QCLASS = IN

        assert_eq!(len, 32);
    }
}
