use memory_stats::memory_stats;
use nix::sys::mman::{MapFlags, ProtFlags};
use rand::distributions::Distribution;
use rand::{distributions::Uniform, RngCore, SeedableRng};
use std::error::Error;
use std::num::NonZeroUsize;
use std::os::raw::c_void;
use std::slice;
use std::time::Instant;

#[cfg(any(test, target_os = "linux"))]
#[macro_use]
extern crate lazy_static;

mod anyos_hugepages;

#[cfg(target_os = "linux")]
mod linux_hugepages;
#[cfg(target_os = "linux")]
use linux_hugepages::madvise_hugepages_on_linux;
#[cfg(target_os = "linux")]
use linux_hugepages::print_hugepage_setting_on_linux;

#[cfg(not(target_os = "linux"))]
mod notlinux_hugepages;
#[cfg(not(target_os = "linux"))]
use notlinux_hugepages::madvise_hugepages_on_linux;
#[cfg(not(target_os = "linux"))]
use notlinux_hugepages::print_hugepage_setting_on_linux;

const FILLED: u64 = 0x42;

fn main() -> Result<(), Box<dyn Error>> {
    const TEST_SIZE_GIB: usize = 4;
    const TEST_SIZE_BYTES: usize = TEST_SIZE_GIB * 1024 * 1024 * 1024;
    const TEST_SIZE_U64: usize = TEST_SIZE_BYTES / 8;

    // the rand book suggests Xoshiro256Plus is fast and pretty good:
    // https://rust-random.github.io/book/guide-rngs.html
    let mut rng = rand_xoshiro::Xoshiro256Plus::from_entropy();

    let mem_before = memory_stats().unwrap();
    let start = Instant::now();
    let mut v = Vec::with_capacity(TEST_SIZE_U64);
    v.resize(TEST_SIZE_U64, FILLED);
    let end = Instant::now();
    let duration = end - start;
    println!(
        "Vec: alloc and filled {TEST_SIZE_GIB} GiB in {duration:?}; {}",
        humanunits::byte_rate_string(TEST_SIZE_BYTES, duration)
    );
    rnd_accesses(&mut rng, &v);
    let mem_after = memory_stats().unwrap();
    println!(
        "RSS before: {}; RSS after: {}; diff: {}\n",
        humanunits::bytes_string(mem_before.physical_mem),
        humanunits::bytes_string(mem_after.physical_mem),
        humanunits::bytes_string(mem_after.physical_mem - mem_before.physical_mem)
    );
    drop(v);

    print_hugepage_setting_on_linux()?;

    let mem_before = memory_stats().unwrap();
    let start = Instant::now();
    let mut v = MmapU64Slice::new_zero(TEST_SIZE_U64)?;
    for value in v.slice_mut().iter_mut() {
        *value = FILLED;
    }
    let end = Instant::now();
    let duration = end - start;
    println!(
        "MmapSlice: alloc and filled {TEST_SIZE_GIB} GiB in {duration:?}; {}",
        humanunits::byte_rate_string(TEST_SIZE_BYTES, duration)
    );
    rnd_accesses(&mut rng, v.slice());
    let mem_after = memory_stats().unwrap();
    println!(
        "RSS before: {}; RSS after: {}; diff: {}",
        humanunits::bytes_string(mem_before.physical_mem),
        humanunits::bytes_string(mem_after.physical_mem),
        humanunits::bytes_string(mem_after.physical_mem - mem_before.physical_mem)
    );
    drop(v);

    Ok(())
}

struct MmapAligned {
    mmap_pointer: *mut c_void,
    aligned_size: usize,
}

impl MmapAligned {
    // argument order is the same as aligned_alloc.
    fn new(alignment: usize, size: usize) -> Result<Self, nix::errno::Errno> {
        // worse case alignment: mmap returns 1 byte off the alignment, we must waste alignment-1 bytes.
        // To ensure we can do this, we request size+alignment bytes.
        // This shouldn't be so bad: untouched pages won't actually be allocated.
        let aligned_size =
            NonZeroUsize::new(size + alignment).expect("BUG: alignment and size must be > 0");

        let mmap_pointer: *mut c_void;
        unsafe {
            mmap_pointer = nix::sys::mman::mmap(
                None,
                aligned_size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_ANONYMOUS | MapFlags::MAP_PRIVATE,
                0,
                0,
            )?;
        }

        let allocation = Self {
            mmap_pointer,
            aligned_size: aligned_size.get(),
        };
        let aligned_pointer = allocation.get_aligned_mut(alignment);
        let allocation_end = mmap_pointer as usize + aligned_size.get();
        assert!(aligned_pointer as usize + size <= allocation_end);

        Ok(allocation)
    }

    fn get_aligned_mut(&self, alignment: usize) -> *mut c_void {
        align_pointer_value(alignment, self.mmap_pointer as usize) as *mut c_void
    }
}

impl Drop for MmapAligned {
    fn drop(&mut self) {
        println!(
            "dropping mmap pointer={:x?} len={}...",
            self.mmap_pointer, self.aligned_size
        );

        unsafe {
            nix::sys::mman::munmap(self.mmap_pointer.cast::<c_void>(), self.aligned_size)
                .expect("BUG: munmap should not fail");
        }
    }
}

fn align_pointer_value(alignment: usize, pointer_value: usize) -> usize {
    // see bit hacks to check if power of two:
    // https://graphics.stanford.edu/~seander/bithacks.html#DetermineIfPowerOf2
    assert_eq!(0, (alignment & (alignment - 1)));
    // round pointer_value up to nearest alignment; assumes there is sufficient space
    let alignment_mask = !(alignment - 1);
    (pointer_value + (alignment - 1)) & alignment_mask
}

struct MmapU64Slice<'a> {
    // MmapAligned unmaps the mapping using the Drop trait but is otherwise not read
    _allocation: MmapAligned,
    slice: &'a mut [u64],
}

impl<'a> MmapU64Slice<'a> {
    fn new_zero(items: usize) -> Result<Self, nix::errno::Errno> {
        const HUGE_2MIB_MASK: usize = (2 << 20) - 1;
        const HUGE_1GIB_ALIGNMENT: usize = 1 << 30;
        const HUGE_1GIB_MASK: usize = HUGE_1GIB_ALIGNMENT - 1;

        let mem_size = items * 8;
        let allocation = MmapAligned::new(HUGE_1GIB_ALIGNMENT, mem_size)?;
        let slice_pointer = allocation.get_aligned_mut(HUGE_1GIB_ALIGNMENT);
        let slice: &mut [u64];
        unsafe {
            slice = slice::from_raw_parts_mut(slice_pointer.cast::<u64>(), items);
        }

        let mut m = Self {
            _allocation: allocation,
            slice,
        };
        madvise_hugepages_on_linux(m.slice);

        let (mmap_pointer, _) = m.mmap_parts();
        let ptr_usize = mmap_pointer as usize;
        println!(
            "mmap aligned returned {mmap_pointer:x?}; aligned to 2MiB (0x{HUGE_2MIB_MASK:x})? {}; aligned to 1GiB (0x{HUGE_1GIB_MASK:x})? {}",
            ptr_usize & HUGE_2MIB_MASK == 0,
            ptr_usize & HUGE_1GIB_MASK == 0
        );
        Ok(m)
    }

    fn slice(&self) -> &[u64] {
        self.slice
    }

    fn slice_mut(&mut self) -> &mut [u64] {
        self.slice
    }

    fn mmap_parts(&mut self) -> (*mut u64, usize) {
        let mmap_pointer = self.slice_mut().as_mut_ptr();
        let mmap_len = self.slice.len() * 8;
        (mmap_pointer, mmap_len)
    }
}

fn rnd_accesses(rng: &mut dyn RngCore, data: &[u64]) {
    const NUM_ACCESSES: usize = 200_000_000;

    let index_distribution = Uniform::from(0..data.len());
    let start = Instant::now();
    for _ in 0..NUM_ACCESSES {
        let index = index_distribution.sample(rng);
        let v = data[index];
        assert_eq!(v, FILLED);
    }
    let end = Instant::now();
    let duration = end - start;
    println!(
        "{NUM_ACCESSES} in {duration:?}; {:.1} accesses/sec",
        NUM_ACCESSES as f64 / duration.as_secs_f64()
    );
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_align_pointer_value() {
        const ONE_GIB: usize = 1 << 30;
        const SEVEN_GIB: usize = 7 * ONE_GIB;
        const EIGHT_GIB: usize = 8 * ONE_GIB;
        assert_eq!(SEVEN_GIB, align_pointer_value(ONE_GIB, SEVEN_GIB));
        assert_eq!(EIGHT_GIB, align_pointer_value(ONE_GIB, SEVEN_GIB + 1));
        assert_eq!(
            EIGHT_GIB,
            align_pointer_value(ONE_GIB, SEVEN_GIB + (ONE_GIB - 1))
        );
        assert_eq!(EIGHT_GIB, align_pointer_value(ONE_GIB, SEVEN_GIB + ONE_GIB));
    }

    #[test]
    fn test_mmap_aligned() {
        const ONE_GIB: usize = 1 << 30;
        const ONE_MIB: usize = 1 << 20;
        let aligned_alloc = MmapAligned::new(ONE_GIB, ONE_MIB).unwrap();
        let aligned_pointer = aligned_alloc.get_aligned_mut(ONE_GIB);

        // check that we can write to the slice
        let slice: &mut [u64];
        unsafe {
            slice = slice::from_raw_parts_mut(aligned_pointer.cast::<u64>(), ONE_MIB / 8);
        }
        slice[0] = 0x42;
        slice[slice.len() - 1] = 0x42;
        assert_eq!(0x42, slice[0]);
        assert_eq!(0, slice[1]);
        assert_eq!(0, slice[slice.len() - 2]);
        assert_eq!(0x42, slice[slice.len() - 1]);
    }
}
