use nix::sys::mman::MmapAdvise;
use std::error::Error;
use std::ffi::c_void;
use std::fs::File;
use std::io::Read;

use crate::anyos_hugepages;

// See: https://www.kernel.org/doc/Documentation/vm/transhuge.txt
const HUGEPAGE_ENABLED_PATH: &str = "/sys/kernel/mm/transparent_hugepage/enabled";

pub fn print_hugepage_setting_on_linux() -> Result<(), Box<dyn Error>> {
    let mut f = File::open(HUGEPAGE_ENABLED_PATH)?;
    let mut v = Vec::new();
    f.read_to_end(&mut v)?;

    let hugepage_setting = anyos_hugepages::anyos::parse_hugepage_enabled(&v)?;
    println!("transparent_hugepage setting: {hugepage_setting}");

    Ok(())
}

pub fn madvise_hugepages_on_linux(slice: &mut [u64]) {
    const HUGEPAGE_FLAGS: MmapAdvise = MmapAdvise::MADV_HUGEPAGE;

    let slice_pointer = slice.as_mut_ptr().cast::<c_void>();
    let slice_byte_len = slice.len() * 8;
    unsafe {
        nix::sys::mman::madvise(slice_pointer, slice_byte_len, HUGEPAGE_FLAGS)
            .expect("BUG: madvise must succeed");
    }

    anyos_hugepages::anyos::touch_pages(slice);
}
