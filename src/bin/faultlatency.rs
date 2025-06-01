use clap::Parser;
use hugepagedemo::MmapRegion;
use std::{
    error::Error,
    time::{Duration, Instant},
};
use time::OffsetDateTime;

/// Control the options for fault latency testing.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct FaultLatencyOptions {
    /// probe the page latency at this interval.
    #[arg(long, default_value = "1s", value_parser(clap_parse_go_duration))]
    test_interval: Duration,

    /// sleep duration between probing the different page sizes.
    // allow(dead_code) for Mac OS X where the option is unused
    //#[allow(dead_code)]
    #[arg(long, default_value = "100ms", value_parser(clap_parse_go_duration))]
    sleep_between_page_sizes: Duration,
}

/// Parses a duration using Go's formats, with the signature required by clap.
fn clap_parse_go_duration(s: &str) -> Result<Duration, String> {
    let result = go_parse_duration::parse_duration(s);
    match result {
        Err(err) => Err(format!("{err:?}")),
        Ok(nanos) => {
            assert!(nanos >= 0);
            Ok(Duration::from_nanos(
                nanos.try_into().expect("BUG: duration must be >= 0"),
            ))
        }
    }
}

pub struct FaultLatency {
    mmap: Duration,
    fault: Duration,
    second_write: Duration,
}

impl FaultLatency {
    const fn new(mmap: Duration, fault: Duration, second_write: Duration) -> Self {
        Self {
            mmap,
            fault,
            second_write,
        }
    }
}

fn fault_4kib() -> Result<FaultLatency, nix::errno::Errno> {
    const PAGE_4KIB: usize = 4 << 10;

    let start = Instant::now();
    let region = MmapRegion::new(PAGE_4KIB)?;
    let mmap_end = Instant::now();
    let u64_pointer = region.get_mut().cast::<u64>();
    unsafe {
        *u64_pointer = 0x42;
    }
    let fault_end = Instant::now();
    unsafe {
        *u64_pointer = 0x43;
    }
    let second_write_end = Instant::now();

    Ok(FaultLatency::new(
        mmap_end - start,
        fault_end - mmap_end,
        second_write_end - fault_end,
    ))
}

#[allow(clippy::similar_names)]
fn main() -> Result<(), Box<dyn Error>> {
    let config = FaultLatencyOptions::parse();

    let mut next = Instant::now() + config.test_interval;
    loop {
        // sleep first: the first maps/faults after a sleep are slower?
        // I suspect weirdness due to CPU power saving states etc
        std::thread::sleep(next - Instant::now());
        next += config.test_interval;

        let timing_4kib = fault_4kib()?;

        #[cfg(target_os = "linux")]
        {
            std::thread::sleep(config.sleep_between_page_sizes);
            let timing_2mib = linux::fault_2mib()?;

            let wallnow = OffsetDateTime::now_utc();
            println!(
                "{wallnow} 4kiB: mmap:{:?} fault:{:?} second_write:{:?};   2MiB: mmap:{:?} fault:{:?} second_write:{:?}",
                timing_4kib.mmap,
                timing_4kib.fault,
                timing_4kib.second_write,
                timing_2mib.mmap,
                timing_2mib.fault,
                timing_2mib.second_write,
            );
        }
        #[cfg(not(target_os = "linux"))]
        {
            let wallnow = OffsetDateTime::now_utc();
            println!(
                "{wallnow} 4kiB: mmap:{:?} fault:{:?} second_write:{:?}",
                timing_4kib.mmap, timing_4kib.fault, timing_4kib.second_write,
            );
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use hugepagedemo::MmapRegion;
    use nix::sys::mman::MmapAdvise;
    use std::ptr::NonNull;
    use std::{ffi::c_void, time::Instant};

    use crate::FaultLatency;

    /// Allocates a memory region with mmap that is aligned with a specific alignment. This can be used
    /// for huge page alignment. It allocates a region of size + alignment, then munmaps the extra.
    /// Unfortunately, the Linux kernel seems to prefer returning
    struct MmapMadviseNoUnmap {
        _region: MmapRegion,
        aligned_pointer: NonNull<c_void>,
    }

    impl MmapMadviseNoUnmap {
        fn new(size: usize) -> Result<Self, nix::errno::Errno> {
            const ALIGNMENT_2MIB: usize = 2 << 20;

            // worse case alignment: mmap returns 1 byte off the alignment, we must waste alignment-1 bytes.
            // To ensure we can do this, we request size+alignment bytes.
            // This shouldn't be so bad: untouched pages won't actually be allocated.
            let align_rounded_size = size + ALIGNMENT_2MIB;
            let region = MmapRegion::new(align_rounded_size)?;

            // Calculate the aligned block, preferring the HIGHEST aligned address,
            // since the kernel seems to allocate consecutive allocations downward.
            // This allows consecutive calls to mmap to be contiguous, which MIGHT
            // allow the kernel to coalesce them into huge pages? Not sure.
            let allocation_end = region.get_mut() as usize + align_rounded_size;
            let aligned_pointer_usize =
                align_pointer_value_down(ALIGNMENT_2MIB, allocation_end - size);

            assert!(region.ptr_as_usize() <= aligned_pointer_usize);
            assert!(aligned_pointer_usize + size <= allocation_end);

            let aligned_pointer = NonNull::new(aligned_pointer_usize as *mut c_void).unwrap();
            unsafe {
                nix::sys::mman::madvise(aligned_pointer, size, MmapAdvise::MADV_HUGEPAGE)
                    .expect("BUG: madvise must succeed");
            }

            Ok(Self {
                _region: region,
                aligned_pointer,
            })
        }

        const fn as_ptr(&self) -> *mut c_void {
            self.aligned_pointer.as_ptr()
        }
    }

    fn align_pointer_value_down(alignment: usize, pointer_value: usize) -> usize {
        // see bit hacks to check if power of two:
        // https://graphics.stanford.edu/~seander/bithacks.html#DetermineIfPowerOf2
        assert_eq!(0, (alignment & (alignment - 1)));
        // round pointer_value down to nearest alignment; assumes there is sufficient space
        let alignment_mask = !(alignment - 1);
        pointer_value & alignment_mask
    }

    pub fn fault_2mib() -> Result<FaultLatency, nix::errno::Errno> {
        const PAGE_2MIB: usize = 2 << 20;

        let start = Instant::now();
        let region = MmapMadviseNoUnmap::new(PAGE_2MIB)?;
        let mmap_end = Instant::now();
        let u64_pointer = region.as_ptr().cast::<u64>();
        unsafe {
            *u64_pointer = 0x42;
        }
        let fault_end = Instant::now();
        unsafe {
            *u64_pointer = 0x43;
        }
        let second_write_end = Instant::now();

        Ok(FaultLatency::new(
            mmap_end - start,
            fault_end - mmap_end,
            second_write_end - fault_end,
        ))
    }
}
