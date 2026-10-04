//! Hands memory the allocator holds for reuse back to the system.

/// Returns the pages the allocator keeps for later allocations. A
/// transcription frees its model and buffers, tens of megabytes to gigabytes,
/// at once. The allocator stops counting those pages in the app's footprint
/// but keeps them resident, so the app looks that much larger until the
/// system runs short of memory and takes them back.
#[cfg(target_os = "macos")]
pub fn release_free_pages() {
    unsafe extern "C" {
        fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
    }

    // Every zone, as much as possible.
    unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
}

#[cfg(not(target_os = "macos"))]
pub fn release_free_pages() {}
