//! Common utilities for DAWN plugins.
//!
//! Provides memory allocation helpers and re-exports for building WASM plugins.

pub use core::slice;
pub use std::vec::Vec;

/// Maximum allowed allocation size in bytes for plugin_alloc.
///
/// Set to the max size of a UDP packet
const MAX_ALLOC_SIZE: u32 = 65535;

/// Allocate memory in WASM linear memory.
///
/// Plugins should re-export this as:
/// ```ignore
/// #[no_mangle]
/// pub extern "C" fn alloc(size: u32) -> *mut u8 {
///     dawn_plugin_common::plugin_alloc(size)
/// }
/// ```
pub fn plugin_alloc(size: u32) -> *mut u8 {
    // Return null for empty or too-large allocations
    if size == 0 || size > MAX_ALLOC_SIZE {
        return core::ptr::null_mut();
    }
    // Allocate empty vec
    let mut buf = vec![0u8; size as usize];
    let ptr = buf.as_mut_ptr();
    // Forget about the vec (without doing the drop)
    core::mem::forget(buf);
    // Return the allocated buffer
    ptr
}

/// Deallocate memory in WASM linear memory.
///
/// # Safety
/// `ptr` must have been allocated by `plugin_alloc` with the same `size`.
///
/// Plugins should re-export this as:
/// ```ignore
/// #[no_mangle]
/// pub unsafe extern "C" fn dealloc(ptr: *mut u8, size: u32) {
///     dawn_plugin_common::plugin_dealloc(ptr, size)
/// }
/// ```
pub unsafe fn plugin_dealloc(ptr: *mut u8, size: u32) {
    // This should never fail because usize==u32 on wasm32
    match usize::try_from(size) {
        // Recreate and deallocate the vec
        Ok(capacity) => drop(Vec::from_raw_parts(ptr, capacity, capacity)),
        Err(err) => unreachable!("Error deallocating: {err}"),
    }
}
