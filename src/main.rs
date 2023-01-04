use core::slice;
use memory_stats::memory_stats;
use nix::libc::uintptr_t;
use nix::sys::mman::{MapFlags, ProtFlags};
#[cfg(any(test, target_os = "linux"))]
use nix::unistd::SysconfVar;
use rand::distributions::Distribution;
use rand::{distributions::Uniform, RngCore, SeedableRng};
use std::error::Error;
use std::num::NonZeroUsize;
use std::os::raw::c_void;
use std::time::Instant;

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
        "RSS before: {}; RSS after: {}; diff: {}",
        humanunits::bytes_string(mem_before.physical_mem),
        humanunits::bytes_string(mem_after.physical_mem),
        humanunits::bytes_string(mem_after.physical_mem - mem_before.physical_mem)
    );
    drop(v);

    let mem_before = memory_stats().unwrap();
    let start = Instant::now();
    let mut v = MmapU64Slice::new_zero(TEST_SIZE_U64)?;
    for value in v.slice_mut().iter_mut() {
        *value = FILLED;
    }
    let end = Instant::now();
    let duration = end - start;
    println!(
        "\nMmapSlice: alloc and filled {TEST_SIZE_GIB} GiB in {duration:?}; {}",
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

struct MmapU64Slice<'a> {
    slice: &'a mut [u64],
}

impl<'a> MmapU64Slice<'a> {
    fn new_zero(items: usize) -> Result<Self, nix::errno::Errno> {
        const HUGE_2MIB_MASK: uintptr_t = (2 << 20) - 1;
        const HUGE_1GIB_MASK: uintptr_t = (1 << 30) - 1;

        let mem_size = NonZeroUsize::new(items * 8).unwrap();

        let pointer: *mut c_void;
        let slice: &mut [u64];
        unsafe {
            pointer = nix::sys::mman::mmap(
                None,
                mem_size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_ANONYMOUS | MapFlags::MAP_PRIVATE,
                0,
                0,
            )?;

            slice = slice::from_raw_parts_mut(pointer.cast::<u64>(), items);
        }

        let mut m = Self { slice };
        m.madvise_hugepages_on_linux();

        let (mmap_pointer, _) = m.mmap_parts();
        let ptr_usize = mmap_pointer as usize;
        println!(
            "mmap returned {mmap_pointer:x?}; aligned to 2MiB (0x{HUGE_2MIB_MASK:x})? {}; aligned to 1GiB (0x{HUGE_1GIB_MASK:x})? {}",
            ptr_usize & HUGE_2MIB_MASK == 0,
            ptr_usize & HUGE_1GIB_MASK == 0
        );
        Ok(m)
    }

    #[cfg(target_os = "linux")]
    fn madvise_hugepages_on_linux(&mut self) {
        use nix::libc::HW_PAGESIZE;

        let (mmap_pointer, mmap_len) = self.mmap_parts();
        let advise_flags = MmapAdvise::MADV_HUGEPAGE;
        nix::sys::mman::madvise(mmap_pointer, mmap_len, advise_flags)
            .expect("BUG: madvise must succeed");

        touch_pages(self.slice);
    }

    // allow unused_self because it is used on Linux
    #[cfg(not(target_os = "linux"))]
    #[allow(clippy::unused_self)]
    fn madvise_hugepages_on_linux(&mut self) {
        // Do nothing if not on Linux
    }

    fn slice(&self) -> &[u64] {
        self.slice
    }

    fn slice_mut(&mut self) -> &mut [u64] {
        self.slice
    }

    fn mmap_parts(&self) -> (*const u64, usize) {
        let mmap_pointer = self.slice().as_ptr();
        let mmap_len = self.slice.len() * 8;
        (mmap_pointer, mmap_len)
    }
}

impl<'a> Drop for MmapU64Slice<'a> {
    fn drop(&mut self) {
        let (mmap_pointer, mmap_len) = self.mmap_parts();
        // println!("dropping mmap pointer={mmap_pointer:x?} len={mmap_len} ...");

        unsafe {
            nix::sys::mman::munmap(mmap_pointer as *mut c_void, mmap_len)
                .expect("BUG: munmap should not fail");
        }
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

#[cfg(any(test, target_os = "linux"))]
fn touch_pages(s: &mut [u64]) {
    let page_size = nix::unistd::sysconf(SysconfVar::PAGE_SIZE)
        .expect("BUG: sysconf(_SC_PAGESIZE) must work")
        .expect("BUG: page size must not be None");
    println!("page_size={page_size}");

    // write a zero every stride elements, which should fault every page
    let stride = page_size as usize / 8;
    for index in (0..s.len()).step_by(stride) {
        s[index] = 0;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_touch_pages() {
        // just tests that it does not crash
        const SIZE: usize = 1024 * 1024;
        let mut v: Vec<u64> = vec![0; SIZE];
        touch_pages(&mut v);
    }
}
