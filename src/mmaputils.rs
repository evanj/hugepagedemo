use nix::sys::mman::{MapFlags, ProtFlags};
use std::{ffi::c_void, num::NonZeroUsize, ptr::NonNull};

/// Owns a memory region with mmap and calls munmap on drop.
pub struct MmapOwner {
    mmap_pointer: NonNull<c_void>,
    size: usize,
}

impl MmapOwner {
    #[must_use]
    pub const fn new(mmap_pointer: NonNull<c_void>, size: usize) -> Self {
        Self { mmap_pointer, size }
    }

    #[must_use]
    pub const fn get_mut(&self) -> *mut c_void {
        self.mmap_pointer.as_ptr()
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
            nix::sys::mman::munmap(self.mmap_pointer, self.size)
                .expect("BUG: munmap should not fail");
        }
    }
}

/// Allocates a new memory region with mmap.
pub struct MmapRegion {
    region: MmapOwner,
}

impl MmapRegion {
    // this is used by faultlatency; clippy doesn't find it?
    #[allow(dead_code)]
    pub fn new(size: usize) -> Result<Self, nix::errno::Errno> {
        Self::new_flags(size, MapFlags::empty())
    }

    pub fn new_flags(size: usize, flags: MapFlags) -> Result<Self, nix::errno::Errno> {
        let mmap_pointer: NonNull<c_void>;
        let non_zero_size = NonZeroUsize::new(size).expect("BUG: size must be > 0");
        unsafe {
            mmap_pointer = nix::sys::mman::mmap_anonymous(
                None,
                non_zero_size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_ANONYMOUS | MapFlags::MAP_PRIVATE | flags,
            )?;
        }

        Ok(Self {
            region: MmapOwner::new(mmap_pointer, size),
        })
    }

    // this is actually used by faultlatency; clippy doesn't find it?
    #[allow(dead_code)]
    #[must_use]
    pub const fn get_mut(&self) -> *mut c_void {
        self.region.get_mut()
    }

    // this is actually used by faultlatency; clippy doesn't find it?
    #[allow(dead_code)]
    #[must_use]
    pub fn ptr_as_usize(&self) -> usize {
        self.region.get_mut() as usize
    }
}
