use crate::anyos_hugepages;
use nix::sys::mman::MmapAdvise;
use std::error::Error;
use std::ffi::c_void;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

// See: https://www.kernel.org/doc/Documentation/vm/transhuge.txt
const HUGEPAGE_ENABLED_PATH: &str = "/sys/kernel/mm/transparent_hugepage/enabled";

pub fn print_hugepage_setting_on_linux() -> Result<(), Box<dyn Error>> {
    let mut f = File::open(HUGEPAGE_ENABLED_PATH)?;
    let mut v = Vec::new();
    f.read_to_end(&mut v)?;

    let hugepage_setting = anyos_hugepages::parse_hugepage_enabled(&v)?;
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

    anyos_hugepages::touch_pages(slice);
}

// See https://www.kernel.org/doc/Documentation/vm/pagemap.txt for
// format which these bitmasks refer to
// #define PAGEMAP_PRESENT(ent) (((ent) & (1ull << 63)) != 0)
// #define PAGEMAP_PFN(ent) ((ent) & ((1ull << 55) - 1))

/// Represents an entry in /proc/self/pagemap documented by:
/// <https://www.kernel.org/doc/Documentation/vm/pagemap.txt>
struct PagemapEntry {
    v: u64,
}

impl PagemapEntry {
    const fn new(v: u64) -> Self {
        Self { v }
    }

    const fn from_bytes(b: [u8; 8]) -> Self {
        Self::new(u64::from_le_bytes(b))
    }

    const fn present(&self) -> bool {
        // bit 63
        self.v & (1 << 63) != 0
    }

    const fn page_frame_number(&self) -> u64 {
        // bit 0-54 inclusive
        const MASK: u64 = (1 << 55) - 1;
        self.v & MASK
    }
}

/// Returns the best guess at the page size for the address pointed at by p.
/// This needs to run as root to work correctly. This function will print
/// detailed debugging output.
pub fn read_page_size(p: usize) -> Result<usize, std::io::Error> {
    const PAGEMAP_PATH: &str = "/proc/self/pagemap";
    const KPAGEFLAGS_PATH: &str = "/proc/kpageflags";

    // KPF_THP https://github.com/torvalds/linux/blob/master/include/uapi/linux/kernel-page-flags.h
    const KPAGEFLAGS_THP_BIT: u64 = 22;

    let mut pagemap_f = File::open(PAGEMAP_PATH)?;

    // Each pagemap entry is 8 bytes / 64 bits
    // There is one entry for each base page size
    // https://www.kernel.org/doc/Documentation/vm/pagemap.txt
    let page_size = anyos_hugepages::sysconf_page_size();
    let offset = p / page_size * 8;
    pagemap_f.seek(SeekFrom::Start(offset as u64))?;

    let mut entry_bytes = [0u8; 8];
    pagemap_f.read_exact(&mut entry_bytes[..])?;
    let entry = PagemapEntry::from_bytes(entry_bytes);
    assert!(entry.present(), "page for p=0x{p:x?} not allocated");

    if entry.page_frame_number() == 0 {
        println!(
            "  page frame number is zero; must run as root outside a container; assuming default page size"
        );
        return Ok(page_size);
    }

    let mut kpageflags_f = File::open(KPAGEFLAGS_PATH)?;
    let offset = entry.page_frame_number() * 8;
    kpageflags_f.seek(SeekFrom::Start(offset))?;
    kpageflags_f.read_exact(&mut entry_bytes)?;

    let kpageflag_entry = u64::from_le_bytes(entry_bytes);
    if (kpageflag_entry & (1 << KPAGEFLAGS_THP_BIT)) == 0 {
        println!("  kpageflags does not have THP bit set; not a huge page");
        return Ok(page_size);
    }

    println!("  kpageflags THP bit is set: is a huge page!");

    // Read the size of the huge page from /sys/kernel/mm/transparent_hugepage/hpage_pmd_size
    read_hugepage_size()
}

fn read_hugepage_size() -> Result<usize, std::io::Error> {
    const HPAGE_PMD_SIZE_PATH: &str = "/sys/kernel/mm/transparent_hugepage/hpage_pmd_size";
    let mut hpage_size_string = std::fs::read_to_string(HPAGE_PMD_SIZE_PATH)?;
    // always terminated by \n
    if hpage_size_string.ends_with('\n') {
        hpage_size_string.pop();
    }

    let hpage_size_result = hpage_size_string.parse::<usize>();
    match hpage_size_result {
        Err(err) => {
            let msg = format!("  failed to parse {HPAGE_PMD_SIZE_PATH}: {err:?}");
            Err(std::io::Error::new(std::io::ErrorKind::Other, msg))
        }
        Ok(hpage_size) => Ok(hpage_size),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod test {
    use super::*;

    #[test]
    fn test_read_hugepage_size() {
        // this is not always true, but true for x86_64 and current aarch64 platforms
        assert_eq!(2048 * 1024, read_hugepage_size().unwrap());
    }
}
