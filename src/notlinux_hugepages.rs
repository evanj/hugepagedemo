/// This file contains stub functions when building this project for non-Linux operating systems.
use std::error::Error;

#[cfg(not(target_os = "linux"))]
#[allow(clippy::unnecessary_wraps)]
pub fn print_hugepage_setting_on_linux() -> Result<(), Box<dyn Error>> {
    println!("not running on linux; no transparent hugepage setting to parse");
    Ok(())
}

pub fn madvise_hugepages_on_linux(_slice: &mut [u64]) {
    // Do nothing if not on linux
    println!("not running on linux; not calling madvise");
}
