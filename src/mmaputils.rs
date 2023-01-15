use std::{ffi::c_void, num::NonZeroUsize};

use nix::sys::mman::{MapFlags, ProtFlags};

/// Owns a memory region with mmap and calls munmap on drop.
pub struct MmapOwner {
    mmap_pointer: *mut c_void,
    size: usize,
}

impl MmapOwner {
    pub const fn new(mmap_pointer: *mut c_void, size: usize) -> Self {
        Self { mmap_pointer, size }
    }

    pub const fn get_mut(&self) -> *mut c_void {
        self.mmap_pointer
    }
}

impl Drop for MmapOwner {
    fn drop(&mut self) {
        // println!(
        //     "dropping mmap pointer=0x{:x?} len={} end=0x{:x}...",
        //     self.mmap_pointer,
        //     self.size,
        //     self.mmap_pointer as usize + self.size
        // );

        unsafe {
            nix::sys::mman::munmap(self.mmap_pointer.cast::<c_void>(), self.size)
                .expect("BUG: munmap should not fail");
        }
    }
}

/// Allocates a new memory region with mmap.
pub struct MmapRegion {
    region: MmapOwner,
}

impl MmapRegion {
    // this is actually used by faultlatency; clippy doesn't find it?
    #[allow(dead_code)]
    pub fn new(size: usize) -> Result<Self, nix::errno::Errno> {
        Self::new_flags(size, MapFlags::empty())
    }

    pub fn new_flags(size: usize, flags: MapFlags) -> Result<Self, nix::errno::Errno> {
        let mmap_pointer: *mut c_void;
        let non_zero_size = NonZeroUsize::new(size).expect("BUG: size must be > 0");
        unsafe {
            mmap_pointer = nix::sys::mman::mmap(
                None,
                non_zero_size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_ANONYMOUS | MapFlags::MAP_PRIVATE | flags,
                0,
                0,
            )?;
        }

        Ok(Self {
            region: MmapOwner::new(mmap_pointer, size),
        })
    }

    #[must_use]
    pub const fn get_mut(&self) -> *mut c_void {
        self.region.get_mut()
    }
}
