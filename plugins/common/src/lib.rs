//! Common utilities for DAWN plugins.
//!
//! Provides memory allocation helpers and re-exports for building WASM plugins.

pub use core::slice;
pub use std::vec::Vec;

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
    let mut buf = vec![0u8; size as usize];
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
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
    let _ = Vec::from_raw_parts(ptr, 0, size as usize);
}
