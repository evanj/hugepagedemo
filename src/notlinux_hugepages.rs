use crate::anyos_hugepages;
use std::error::Error;

#[allow(clippy::unnecessary_wraps)]
pub fn print_hugepage_setting_on_linux() -> Result<(), Box<dyn Error>> {
    println!("not running on linux; no transparent hugepage setting to parse");
    Ok(())
}

pub fn madvise_hugepages_on_linux(_slice: &mut [u64]) {
    // Do nothing if not on linux
    println!("not running on linux; not calling madvise");
}

#[allow(clippy::unnecessary_wraps)]
pub fn read_page_size(_p: usize) -> Result<usize, std::io::Error> {
    println!("not running on linux; assuming allocation size = default page size");
    let page_size = anyos_hugepages::sysconf_page_size();
    Ok(page_size)
}
