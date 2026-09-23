//! File names: validation, 8.3 short names (basis-name generation with numeric tails as in the
//! Microsoft FAT specification), VFAT checksums and case-insensitive comparison.

use crate::{Error, Result};
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

/// Longest long file name in UTF-16 code units.
pub(crate) const MAX_NAME_UNITS: usize = 255;

/// Short-name NT case flags (byte 12 of a directory entry).
pub(crate) const NT_LOWER_BASE: u8 = 0x08;
pub(crate) const NT_LOWER_EXT: u8 = 0x10;

/// Code page 437, bytes 0x80..=0xFF (the OEM code page used for short names).
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Converts one OEM (code page 437) byte to a character.
pub(crate) fn oem_to_char(byte: u8) -> char {
    if byte < 0x80 { byte as char } else { CP437_HIGH[(byte - 0x80) as usize] }
}

/// Characters never allowed in a long file name (besides control characters).
fn is_forbidden_long(ch: char) -> bool {
    (ch as u32) < 0x20 || ch == '\u{7f}' || matches!(ch, '"' | '*' | '/' | ':' | '<' | '>' | '?' | '\\' | '|')
}

/// Characters allowed in a short name as stored (upper-case letters, digits and a few symbols).
pub(crate) fn is_valid_short_char(byte: u8) -> bool {
    matches!(byte,
        b'A'..=b'Z' | b'0'..=b'9' | b'$' | b'%' | b'\'' | b'-' | b'_' | b'@' | b'~' | b'`' | b'!'
        | b'(' | b')' | b'{' | b'}' | b'^' | b'#' | b'&')
        || byte >= 0x80
}

/// Checks a name for a new directory entry.
pub(crate) fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(Error::InvalidName);
    }
    if name.encode_utf16().count() > MAX_NAME_UNITS {
        return Err(Error::InvalidName);
    }
    if name.chars().any(is_forbidden_long) {
        return Err(Error::InvalidName);
    }
    // Windows silently strips trailing dots and spaces, which would make the stored name differ
    // from the requested one; refuse such names instead.
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(Error::InvalidName);
    }
    Ok(())
}

/// Case-insensitive comparison using Unicode upper-case mapping.
pub(crate) fn eq_ignore_case(a: &str, b: &str) -> bool {
    if a.len() == b.len() && a.eq_ignore_ascii_case(b) {
        return true;
    }
    let mut left = a.chars().flat_map(char::to_uppercase);
    let mut right = b.chars().flat_map(char::to_uppercase);
    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x == y => {}
            _ => return false,
        }
    }
}

/// Upper-case form of a name, used as a key for duplicate detection.
pub(crate) fn upper_key(name: &str) -> String {
    name.chars().flat_map(char::to_uppercase).collect()
}

/// Display form of an 11-byte short name, honoring the NT lower-case flags.
pub(crate) fn short_name_to_string(raw: &[u8; 11], nt_flags: u8) -> String {
    let mut out = String::new();
    let base_end = raw[..8].iter().rposition(|&b| b != b' ').map_or(0, |i| i + 1);
    let ext_end = raw[8..].iter().rposition(|&b| b != b' ').map_or(0, |i| i + 1);
    for (i, &byte) in raw[..base_end].iter().enumerate() {
        let byte = if i == 0 && byte == 0x05 { 0xE5 } else { byte };
        let ch = oem_to_char(byte);
        out.push(if nt_flags & NT_LOWER_BASE != 0 { ch.to_ascii_lowercase() } else { ch });
    }
    if ext_end > 0 {
        out.push('.');
        for &byte in &raw[8..8 + ext_end] {
            let ch = oem_to_char(byte);
            out.push(if nt_flags & NT_LOWER_EXT != 0 { ch.to_ascii_lowercase() } else { ch });
        }
    }
    out
}

/// VFAT checksum of a short name, stored in each of its long-name entries.
pub(crate) fn lfn_checksum(short: &[u8; 11]) -> u8 {
    short.iter().fold(0u8, |sum, &byte| ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(byte))
}

/// The basis name of a long name (fatgen103, "Basis-Name Generation Algorithm").
#[derive(Debug, Clone)]
pub(crate) struct Basis {
    primary: Vec<u8>,
    extension: Vec<u8>,
    /// The long name maps to the basis without loss (no numeric tail is needed).
    fits: bool,
    /// The long name *is* the short name (upper-case 8.3): no long-name entries are needed.
    exact: bool,
}

impl Basis {
    pub fn new(name: &str) -> Basis {
        let mut lossy = false;
        let mut mapped: Vec<u8> = Vec::with_capacity(name.len());
        for ch in name.chars() {
            let upper = ch.to_ascii_uppercase();
            if upper == '.' || upper == ' ' || (upper.is_ascii() && is_valid_short_char(upper as u8)) {
                mapped.push(upper as u8);
            } else {
                lossy = true;
                mapped.push(b'_');
            }
        }
        // Strip all spaces and leading periods.
        let stripped: Vec<u8> = mapped.iter().copied().filter(|&b| b != b' ').collect();
        let leading_dots = stripped.iter().take_while(|&&b| b == b'.').count();
        let stripped = &stripped[leading_dots..];
        let primary: Vec<u8> = stripped.iter().copied().take_while(|&b| b != b'.').take(8).collect();
        let extension: Vec<u8> = match stripped.iter().rposition(|&b| b == b'.') {
            Some(dot) => stripped[dot + 1..].iter().copied().filter(|&b| b != b'.').take(3).collect(),
            None => Vec::new(),
        };
        let primary = if primary.is_empty() { alloc::vec![b'_'] } else { primary };
        let mut formatted = primary.clone();
        if !extension.is_empty() {
            formatted.push(b'.');
            formatted.extend_from_slice(&extension);
        }
        let fits = !lossy && formatted == mapped;
        let exact = fits && formatted == name.as_bytes();
        Basis { primary, extension, fits, exact }
    }

    /// The basis as an 11-byte short name, optionally with a `~n` numeric tail.
    pub fn short_name(&self, tail: Option<u32>) -> [u8; 11] {
        let mut raw = [b' '; 11];
        let mut primary = self.primary.clone();
        if let Some(n) = tail {
            let mut digits = Vec::new();
            let mut value = n;
            loop {
                digits.push(b'0' + (value % 10) as u8);
                value /= 10;
                if value == 0 {
                    break;
                }
            }
            digits.push(b'~');
            digits.reverse();
            primary.truncate(8 - digits.len());
            primary.extend_from_slice(&digits);
        }
        raw[..primary.len()].copy_from_slice(&primary);
        raw[8..8 + self.extension.len()].copy_from_slice(&self.extension);
        raw
    }

    pub fn needs_long_name(&self) -> bool {
        !self.exact
    }

    /// Whether the name maps to its short form without loss.
    pub fn fits(&self) -> bool {
        self.fits
    }
}

/// Picks a short name for `name` that does not collide with `existing`. Returns the short name
/// and whether long-name entries are needed.
pub(crate) fn generate_short_name(name: &str, existing: &BTreeSet<[u8; 11]>) -> Result<([u8; 11], bool)> {
    let basis = Basis::new(name);
    if basis.fits {
        let short = basis.short_name(None);
        if !existing.contains(&short) {
            return Ok((short, basis.needs_long_name()));
        }
    }
    for n in 1..=999_999u32 {
        let short = basis.short_name(Some(n));
        if !existing.contains(&short) {
            return Ok((short, true));
        }
    }
    Err(Error::NoSpace)
}

/// Converts a volume label to its 11-byte on-disk form (upper-case, space padded).
pub(crate) fn label_bytes(label: &str) -> Result<[u8; 11]> {
    let mut raw = [b' '; 11];
    if label.len() > 11 || !label.is_ascii() {
        return Err(Error::InvalidName);
    }
    for (i, byte) in label.bytes().enumerate() {
        let upper = byte.to_ascii_uppercase();
        if upper != b' ' && !is_valid_short_char(upper) {
            return Err(Error::InvalidName);
        }
        raw[i] = upper;
    }
    if label.starts_with(' ') {
        return Err(Error::InvalidName);
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short(s: &str) -> [u8; 11] {
        let mut raw = [b' '; 11];
        let (base, ext) = s.split_once('.').unwrap_or((s, ""));
        raw[..base.len()].copy_from_slice(base.as_bytes());
        raw[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
        raw
    }

    fn generate(name: &str, existing: &[&str]) -> ([u8; 11], bool) {
        let set = existing.iter().map(|s| short(s)).collect();
        generate_short_name(name, &set).unwrap()
    }

    #[test]
    fn exact_short_names_need_no_long_name() {
        assert_eq!(generate("README.TXT", &[]), (short("README.TXT"), false));
        assert_eq!(generate("A", &[]), (short("A"), false));
        assert_eq!(generate("12345678.123", &[]), (short("12345678.123"), false));
        assert_eq!(generate("A-B_C", &[]), (short("A-B_C"), false));
    }

    #[test]
    fn lowercase_keeps_basis_but_needs_long_name() {
        assert_eq!(generate("readme.txt", &[]), (short("README.TXT"), true));
        assert_eq!(generate("Readme.Txt", &[]), (short("README.TXT"), true));
        // Collides with an existing short name: gets a tail.
        assert_eq!(generate("readme.txt", &["README.TXT"]), (short("README~1.TXT"), true));
    }

    #[test]
    fn lossy_and_long_names_get_tails() {
        assert_eq!(generate("long file name.txt", &[]), (short("LONGFI~1.TXT"), true));
        assert_eq!(generate("a+b.txt", &[]), (short("A_B~1.TXT"), true));
        assert_eq!(generate(".bashrc", &[]), (short("BASHRC~1"), true));
        assert_eq!(generate("archive.tar.gz", &[]), (short("ARCHIV~1.GZ"), true));
        assert_eq!(generate("x.html", &[]), (short("X~1.HTM"), true));
        assert_eq!(generate("über.txt", &[]), (short("_BER~1.TXT"), true));
        assert_eq!(generate("日本語", &[]), (short("___~1"), true));
        assert_eq!(generate("a b", &[]), (short("AB~1"), true));
    }

    #[test]
    fn tails_avoid_collisions() {
        let mut existing = BTreeSet::new();
        for i in 1..=20 {
            let name = alloc::format!("long file name {}.txt", i);
            let (s, lfn) = generate_short_name(&name, &existing).unwrap();
            assert!(lfn);
            assert!(existing.insert(s), "duplicate short name for {}", name);
        }
        assert!(existing.contains(&short("LONGFI~1.TXT")));
        assert!(existing.contains(&short("LONGFI~9.TXT")));
        assert!(existing.contains(&short("LONGF~10.TXT")));
        assert!(existing.contains(&short("LONGF~20.TXT")));
        let (s, _) = generate("long file name 21.txt", &[]);
        assert_eq!(s, short("LONGFI~1.TXT"));
    }

    #[test]
    fn tail_numbers_shrink_the_primary() {
        let basis = Basis::new("abcdefghijkl.txt");
        assert_eq!(basis.short_name(Some(1)), short("ABCDEF~1.TXT"));
        assert_eq!(basis.short_name(Some(123456)), short("A~123456.TXT"));
    }

    #[test]
    fn checksum_matches_reference() {
        // Reference value computed with the algorithm from the FAT specification.
        let raw = short("LONGFI~1.TXT");
        let mut sum: u8 = 0;
        for &b in raw.iter() {
            sum = (if sum & 1 != 0 { 0x80u8 } else { 0 }).wrapping_add(sum >> 1).wrapping_add(b);
        }
        assert_eq!(lfn_checksum(&raw), sum);
        assert_eq!(lfn_checksum(&short("README.TXT")), lfn_checksum(&short("README.TXT")));
    }

    #[test]
    fn validation() {
        for bad in
            ["", ".", "..", "a/b", "a\\b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "a\u{1}b", "end.", "end "]
        {
            assert_eq!(validate_name(bad), Err(Error::InvalidName), "{:?}", bad);
        }
        for good in ["a", "A.B.C", " leading", "semi;colon", "plus+equals=", "[brackets]", "ünïcödé", ".hidden", "..x"]
        {
            assert_eq!(validate_name(good), Ok(()), "{:?}", good);
        }
        let long: String = "x".repeat(255);
        assert_eq!(validate_name(&long), Ok(()));
        let too_long: String = "x".repeat(256);
        assert_eq!(validate_name(&too_long), Err(Error::InvalidName));
        // 128 characters outside the BMP are 256 UTF-16 units.
        let astral: String = "😀".repeat(128);
        assert_eq!(validate_name(&astral), Err(Error::InvalidName));
    }

    #[test]
    fn case_insensitive_compare() {
        assert!(eq_ignore_case("Hello.TXT", "hello.txt"));
        assert!(eq_ignore_case("ÜBER", "über"));
        assert!(!eq_ignore_case("abc", "abd"));
        assert!(!eq_ignore_case("abc", "abcd"));
    }

    #[test]
    fn short_name_display() {
        assert_eq!(short_name_to_string(&short("README.TXT"), 0), "README.TXT");
        assert_eq!(short_name_to_string(&short("README.TXT"), NT_LOWER_BASE), "readme.TXT");
        assert_eq!(short_name_to_string(&short("README.TXT"), NT_LOWER_BASE | NT_LOWER_EXT), "readme.txt");
        assert_eq!(short_name_to_string(&short("NOEXT"), 0), "NOEXT");
        let mut escaped = short("XBC");
        escaped[0] = 0x05;
        assert_eq!(short_name_to_string(&escaped, 0), "σBC");
    }

    #[test]
    fn labels() {
        assert_eq!(&label_bytes("data").unwrap(), b"DATA       ");
        assert_eq!(&label_bytes("MY DISK").unwrap(), b"MY DISK    ");
        assert_eq!(label_bytes("toolonglabel1"), Err(Error::InvalidName));
        assert_eq!(label_bytes("bad*"), Err(Error::InvalidName));
        assert_eq!(label_bytes("ümlaut"), Err(Error::InvalidName));
    }
}
