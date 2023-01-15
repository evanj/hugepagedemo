use hugepagedemo::MmapOwner;
use memory_stats::memory_stats;
use nix::sys::mman::{MapFlags, ProtFlags};
use rand::distributions::Distribution;
use rand::{distributions::Uniform, RngCore, SeedableRng};
use std::error::Error;
use std::num::NonZeroUsize;
use std::os::raw::c_void;
use std::slice;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[cfg(any(test, target_os = "linux"))]
#[macro_use]
extern crate lazy_static;

mod anyos_hugepages;
mod mmaputils;
use mmaputils::MmapRegion;

#[cfg(target_os = "linux")]
mod linux_hugepages;
#[cfg(target_os = "linux")]
use linux_hugepages::madvise_hugepages_on_linux;
#[cfg(target_os = "linux")]
use linux_hugepages::print_hugepage_setting_on_linux;
#[cfg(target_os = "linux")]
use linux_hugepages::read_page_size;

#[cfg(not(target_os = "linux"))]
mod notlinux_hugepages;
#[cfg(not(target_os = "linux"))]
use notlinux_hugepages::madvise_hugepages_on_linux;
#[cfg(not(target_os = "linux"))]
use notlinux_hugepages::print_hugepage_setting_on_linux;
#[cfg(not(target_os = "linux"))]
use notlinux_hugepages::read_page_size;

const FILLED: u64 = 0x42;

#[derive(argh::FromArgs)]
/// Control the options for the huge page demo.
struct HugePageDemoOptions {
    /// disable using mmap with madvise.
    #[argh(option, default = "RunMode::All")]
    run_mode: RunMode,

    /// sleep for 60 seconds before dropping the mmap, to allow examining the process state.
    #[argh(switch)]
    sleep_before_drop: bool,
}

#[derive(strum::EnumString, Eq, PartialEq)]
enum RunMode {
    All,
    VecOnly,
    MmapOnly,
    MmapHugeTLB1GiBOnly,
}

fn main() -> Result<(), Box<dyn Error>> {
    const TEST_SIZE_GIB: usize = 4;
    const TEST_SIZE_BYTES: usize = TEST_SIZE_GIB * 1024 * 1024 * 1024;
    const TEST_SIZE_U64: usize = TEST_SIZE_BYTES / 8;

    let options: HugePageDemoOptions = argh::from_env();

    // the rand book suggests Xoshiro256Plus is fast and pretty good:
    // https://rust-random.github.io/book/guide-rngs.html
    let mut rng = rand_xoshiro::Xoshiro256Plus::from_entropy();

    let mem_before = memory_stats().unwrap();
    if options.run_mode == RunMode::All || options.run_mode == RunMode::VecOnly {
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
    }

    if options.run_mode == RunMode::All || options.run_mode == RunMode::MmapOnly {
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
        let page_size = read_page_size(v.slice().as_ptr() as usize)?;
        println!("  slice page size = {page_size}");

        rnd_accesses(&mut rng, v.slice());
        let mem_after = memory_stats().unwrap();
        println!(
            "RSS before: {}; RSS after: {}; diff: {}",
            humanunits::bytes_string(mem_before.physical_mem),
            humanunits::bytes_string(mem_after.physical_mem),
            humanunits::bytes_string(mem_after.physical_mem - mem_before.physical_mem)
        );

        if options.sleep_before_drop {
            const SLEEP_DURATION: Duration = Duration::from_secs(60);
            println!("sleeping ...");
            sleep(SLEEP_DURATION);
            println!("v[0]={}", v.slice()[0]);
        }

        drop(v);

        let mem_after_drop = memory_stats().unwrap();
        println!(
            "After drop: RSS before: {}; RSS after: {}; diff: {}",
            humanunits::bytes_string(mem_before.physical_mem),
            humanunits::bytes_string(mem_after_drop.physical_mem),
            humanunits::bytes_string(mem_after_drop.physical_mem - mem_before.physical_mem)
        );
    }

    if options.run_mode == RunMode::All || options.run_mode == RunMode::MmapHugeTLB1GiBOnly {
        let mem_before = memory_stats().unwrap();
        let start = Instant::now();
        let mut v = match MmapU64SliceUnaligned::new_zero_flags(
            TEST_SIZE_U64,
            MapFlags::MAP_HUGETLB | MapFlags::MAP_HUGE_1GB,
        ) {
            Ok(v) => v,
            Err(nix::Error::ENOMEM) => {
                println!("ENOMEM: try reserving huge pages with: echo {} | sudo tee /sys/kernel/mm/hugepages/hugepages-1048576kB/nr_hugepages", TEST_SIZE_U64*8/(1<<30));
                return Err(Box::from(nix::Error::ENOMEM));
            }
            Err(err) => {
                return Err(Box::from(err));
            }
        };
        for value in v.slice_mut().iter_mut() {
            *value = FILLED;
        }
        let end = Instant::now();
        let duration = end - start;
        println!(
            "hugetlb 1GiB MmapSlice: alloc and filled {TEST_SIZE_GIB} GiB in {duration:?}; {}",
            humanunits::byte_rate_string(TEST_SIZE_BYTES, duration)
        );
        let page_size = read_page_size(v.slice().as_ptr() as usize)?;
        println!("  slice page size = {page_size}");

        rnd_accesses(&mut rng, v.slice());
        let mem_after = memory_stats().unwrap();
        println!(
            "RSS before: {}; RSS after: {}; diff: {}",
            humanunits::bytes_string(mem_before.physical_mem),
            humanunits::bytes_string(mem_after.physical_mem),
            humanunits::bytes_string(mem_after.physical_mem - mem_before.physical_mem)
        );

        if options.sleep_before_drop {
            const SLEEP_DURATION: Duration = Duration::from_secs(60);
            println!("sleeping ...");
            sleep(SLEEP_DURATION);
            println!("v[0]={}", v.slice()[0]);
        }

        drop(v);

        let mem_after_drop = memory_stats().unwrap();
        println!(
            "After drop: RSS before: {}; RSS after: {}; diff: {}",
            humanunits::bytes_string(mem_before.physical_mem),
            humanunits::bytes_string(mem_after_drop.physical_mem),
            humanunits::bytes_string(mem_after_drop.physical_mem - mem_before.physical_mem)
        );
    }

    Ok(())
}

/// Allocates a memory region with mmap that is aligned with a specific alignment. This can be used
/// for huge page alignment. It allocates a region of size + alignment, then munmaps the extra.
/// Unfortunately, the Linux kernel seems to prefer returning
struct MmapHugeMadviseAligned {
    region: MmapOwner,
}

impl MmapHugeMadviseAligned {
    // argument order is the same as aligned_alloc.
    #[cfg(test)]
    fn new(alignment: usize, size: usize) -> Result<Self, nix::errno::Errno> {
        Self::new_flags(alignment, size, MapFlags::empty())
    }

    // argument order is the same as aligned_alloc.
    fn new_flags(
        alignment: usize,
        size: usize,
        flags: MapFlags,
    ) -> Result<Self, nix::errno::Errno> {
        // worse case alignment: mmap returns 1 byte off the alignment, we must waste alignment-1 bytes.
        // To ensure we can do this, we request size+alignment bytes.
        // This shouldn't be so bad: untouched pages won't actually be allocated.
        let align_rounded_size =
            NonZeroUsize::new(size + alignment).expect("BUG: alignment and size must be > 0");

        let mmap_pointer: *mut c_void;
        unsafe {
            mmap_pointer = nix::sys::mman::mmap(
                None,
                align_rounded_size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_ANONYMOUS | MapFlags::MAP_PRIVATE | flags,
                0,
                0,
            )?;
        }

        // Calculate the aligned block, preferring the HIGHEST aligned address,
        // since the kernel seems to allocate consecutive allocations downward.
        // This allows consecutive calls to mmap to be contiguous, which MIGHT
        // allow the kernel to coalesce them into huge pages? Not sure.
        let allocation_end = mmap_pointer as usize + align_rounded_size.get();
        let aligned_pointer =
            align_pointer_value_down(alignment, allocation_end - size) as *mut c_void;
        // alternative of taking the lowest aligned address
        // let aligned_pointer =
        //     align_pointer_value_up(alignment, mmap_pointer as usize) as *mut c_void;

        assert!(mmap_pointer <= aligned_pointer);
        assert!(aligned_pointer as usize + size <= allocation_end);

        let unaligned_below_size = aligned_pointer as usize - mmap_pointer as usize;
        let aligned_end = aligned_pointer as usize + size;
        let unaligned_above_size = allocation_end - aligned_end;
        // println!(
        //     "mmap_pointer:0x{:x} - unaligned_below_end:0x{:x}; aligned_pointer:0x{:x} - aligned_end:0x{:x}; unaligned_above_end:0x{:x}",
        //     mmap_pointer as usize,
        //     mmap_pointer as usize + unaligned_below_size,
        //     aligned_pointer as usize,
        //     aligned_end,
        //     aligned_end + unaligned_above_size,
        // );

        // if there is an unused section BELOW the allocation: unmap it
        if aligned_pointer != mmap_pointer {
            let unaligned_size = aligned_pointer as usize - mmap_pointer as usize;
            unsafe {
                nix::sys::mman::munmap(mmap_pointer, unaligned_size)
                    .expect("BUG: munmap must succeed");
            }
        }

        // if there is an unused section ABOVE the allocation: unmap it
        if unaligned_above_size != 0 {
            unsafe {
                nix::sys::mman::munmap(aligned_end as *mut c_void, unaligned_above_size)
                    .expect("BUG: munmap must succeed");
            }
        }

        assert_eq!(
            unaligned_below_size + unaligned_above_size + size,
            align_rounded_size.get()
        );

        Ok(Self {
            region: MmapOwner::new(aligned_pointer, size),
        })
    }

    const fn get_mut(&self) -> *mut c_void {
        self.region.get_mut()
    }
}

#[cfg(test)]
fn align_pointer_value_up(alignment: usize, pointer_value: usize) -> usize {
    // see bit hacks to check if power of two:
    // https://graphics.stanford.edu/~seander/bithacks.html#DetermineIfPowerOf2
    assert_eq!(0, (alignment & (alignment - 1)));
    // round pointer_value up to nearest alignment; assumes there is sufficient space
    let alignment_mask = !(alignment - 1);
    (pointer_value + (alignment - 1)) & alignment_mask
}

fn align_pointer_value_down(alignment: usize, pointer_value: usize) -> usize {
    // see bit hacks to check if power of two:
    // https://graphics.stanford.edu/~seander/bithacks.html#DetermineIfPowerOf2
    assert_eq!(0, (alignment & (alignment - 1)));
    // round pointer_value down to nearest alignment; assumes there is sufficient space
    let alignment_mask = !(alignment - 1);
    pointer_value & alignment_mask
}

struct MmapU64Slice<'a> {
    // MmapAligned unmaps the mapping using the Drop trait but is otherwise not read
    _allocation: MmapHugeMadviseAligned,
    slice: &'a mut [u64],
}

impl<'a> MmapU64Slice<'a> {
    fn new_zero(items: usize) -> Result<Self, nix::errno::Errno> {
        Self::new_zero_flags(items, MapFlags::empty())
    }

    fn new_zero_flags(items: usize, flags: MapFlags) -> Result<Self, nix::errno::Errno> {
        const HUGE_2MIB_ALIGNMENT: usize = 2 << 20;
        const HUGE_2MIB_MASK: usize = HUGE_2MIB_ALIGNMENT - 1;
        const HUGE_1GIB_ALIGNMENT: usize = 1 << 30;
        const HUGE_1GIB_MASK: usize = HUGE_1GIB_ALIGNMENT - 1;

        let mem_size = items * 8;
        let allocation = MmapHugeMadviseAligned::new_flags(HUGE_2MIB_ALIGNMENT, mem_size, flags)?;
        let slice_pointer = allocation.get_mut();
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

struct MmapU64SliceUnaligned<'a> {
    // MmapAligned unmaps the mapping using the Drop trait but is otherwise not read
    _allocation: MmapRegion,
    slice: &'a mut [u64],
}

impl<'a> MmapU64SliceUnaligned<'a> {
    fn new_zero_flags(items: usize, flags: MapFlags) -> Result<Self, nix::errno::Errno> {
        const HUGE_2MIB_ALIGNMENT: usize = 2 << 20;
        const HUGE_2MIB_MASK: usize = HUGE_2MIB_ALIGNMENT - 1;
        const HUGE_1GIB_ALIGNMENT: usize = 1 << 30;
        const HUGE_1GIB_MASK: usize = HUGE_1GIB_ALIGNMENT - 1;

        let mem_size = items * 8;
        let allocation = MmapRegion::new_flags(mem_size, flags)?;
        let slice_pointer = allocation.get_mut();
        let slice: &mut [u64];
        unsafe {
            slice = slice::from_raw_parts_mut(slice_pointer.cast::<u64>(), items);
        }

        let mut m = Self {
            _allocation: allocation,
            slice,
        };

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
        "{NUM_ACCESSES} accesses in {duration:?}; {:.1} accesses/sec",
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
        assert_eq!(SEVEN_GIB, align_pointer_value_up(ONE_GIB, SEVEN_GIB));
        assert_eq!(EIGHT_GIB, align_pointer_value_up(ONE_GIB, SEVEN_GIB + 1));
        assert_eq!(
            EIGHT_GIB,
            align_pointer_value_up(ONE_GIB, SEVEN_GIB + (ONE_GIB - 1))
        );
        assert_eq!(
            EIGHT_GIB,
            align_pointer_value_up(ONE_GIB, SEVEN_GIB + ONE_GIB)
        );
    }

    #[test]
    fn test_align_pointer_value_down() {
        const ONE_GIB: usize = 1 << 30;
        const SEVEN_GIB: usize = 7 * ONE_GIB;
        const EIGHT_GIB: usize = 8 * ONE_GIB;
        assert_eq!(SEVEN_GIB, align_pointer_value_down(ONE_GIB, SEVEN_GIB));
        assert_eq!(SEVEN_GIB, align_pointer_value_down(ONE_GIB, SEVEN_GIB + 1));
        assert_eq!(
            SEVEN_GIB,
            align_pointer_value_down(ONE_GIB, SEVEN_GIB + (ONE_GIB - 1))
        );
        assert_eq!(
            EIGHT_GIB,
            align_pointer_value_down(ONE_GIB, SEVEN_GIB + ONE_GIB)
        );
    }

    #[test]
    fn test_mmap_aligned() {
        const ONE_GIB: usize = 1 << 30;
        const ONE_MIB: usize = 1 << 20;

        // repeat a few times to try to trigger bad behavior
        let mut v = Vec::new();
        for _ in 0..10 {
            let aligned_alloc = MmapHugeMadviseAligned::new(ONE_GIB, ONE_MIB).unwrap();
            let aligned_pointer = aligned_alloc.get_mut();

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

            v.push(aligned_alloc);
        }
    }
}
