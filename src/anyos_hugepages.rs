use nix::unistd::SysconfVar;
#[cfg(any(test, target_os = "linux"))]
use std::sync::LazyLock;

#[cfg(any(test, target_os = "linux"))]
#[derive(PartialEq, Eq, Debug)]
pub enum HugepageSetting {
    Always,
    MAdvise,
    Never,
}

#[cfg(any(test, target_os = "linux"))]
impl HugepageSetting {
    fn from_bytes(input: &[u8]) -> Result<Self, String> {
        match input {
            b"always" => Ok(Self::Always),
            b"madvise" => Ok(Self::MAdvise),
            b"never" => Ok(Self::Never),
            _ => Err(format!(
                "unknown transparent_hugepage setting {}",
                String::from_utf8_lossy(input)
            )),
        }
    }
}

#[cfg(any(test, target_os = "linux"))]
impl std::fmt::Display for HugepageSetting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Always => "always",
            Self::MAdvise => "madvise",
            Self::Never => "never",
        };
        write!(f, "{s}")
    }
}

#[cfg(any(test, target_os = "linux"))]
pub fn parse_hugepage_enabled(input: &[u8]) -> Result<HugepageSetting, String> {
    static RE: LazyLock<regex::bytes::Regex> =
        LazyLock::new(|| regex::bytes::Regex::new(r"\[([^\]]+)\]").unwrap());

    let string_matches = RE.captures(input);
    if string_matches.is_none() {
        return Err(format!(
            "could not match hugepages input: {}",
            String::from_utf8_lossy(input)
        ));
    }
    let matched = string_matches.unwrap().get(1).unwrap();

    HugepageSetting::from_bytes(matched.as_bytes())
}

pub fn sysconf_page_size() -> usize {
    let page_size = nix::unistd::sysconf(SysconfVar::PAGE_SIZE)
        .expect("BUG: sysconf(_SC_PAGESIZE) must work")
        .expect("BUG: page size must not be None");
    assert!(page_size > 0, "page_size={page_size} must be > 0");
    page_size as usize
}

#[cfg(any(test, target_os = "linux"))]
pub fn touch_pages(s: &mut [u64]) {
    let page_size = sysconf_page_size();
    println!("touch_pages with page_size={page_size}");

    // write a zero every stride elements, which should fault every page
    let stride = page_size / 8;
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

    #[test]
    fn test_parse_hugepage() {
        assert_eq!(
            HugepageSetting::MAdvise,
            parse_hugepage_enabled(b"always [madvise] never\n").unwrap()
        );
        assert_eq!(
            HugepageSetting::Never,
            parse_hugepage_enabled(b"always madvise [never]\n").unwrap()
        );
    }
}
