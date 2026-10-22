//! Minimal ZIP reading (stored + deflate) over flate2's raw inflater,
//! plus plain-text extraction from Office Open XML (docx/xlsx) and a naive
//! PDF text scan. No system tools, no network.

use flate2::read::DeflateDecoder;
use std::io::Read as _;

pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
}

/// Parse a ZIP file's central directory and inflate every entry.
pub fn zip_entries(bytes: &[u8]) -> Result<Vec<ZipEntry>, String> {
    // Locate the End Of Central Directory record (scan backwards; comment may follow).
    let min = 22usize;
    if bytes.len() < min + 4 {
        return Err("فایل zip معتبر نیست (خیلی کوچک است)".into());
    }
    let limit = bytes.len().saturating_sub(min).saturating_sub(65536);
    let mut eocd = None;
    for i in (limit..bytes.len() - min + 1).rev() {
        if &bytes[i..i + 4] == b"PK\x05\x06" {
            eocd = Some(i);
            break;
        }
    }
    let eocd = eocd.ok_or("EOCD یافت نشد (فایل zip معتبر نیست)")?;
    let count = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]) as usize;
    let cd_off = u32::from_le_bytes([bytes[eocd + 16], bytes[eocd + 17], bytes[eocd + 18], bytes[eocd + 19]]) as usize;
    let mut pos = cd_off;
    let mut out = Vec::new();
    for _ in 0..count {
        if pos + 46 > bytes.len() || &bytes[pos..pos + 4] != b"PK\x01\x02" {
            return Err("مدرک کاتالوگ zip نامعتبر است".into());
        }
        let method = u16::from_le_bytes([bytes[pos + 10], bytes[pos + 11]]);
        let comp_size = u32::from_le_bytes([bytes[pos + 20], bytes[pos + 21], bytes[pos + 22], bytes[pos + 23]]) as usize;
        let name_len = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[pos + 30], bytes[pos + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[pos + 32], bytes[pos + 33]]) as usize;
        let local_off = u32::from_le_bytes([bytes[pos + 42], bytes[pos + 43], bytes[pos + 44], bytes[pos + 45]]) as usize;
        let name = String::from_utf8_lossy(&bytes[pos + 46..pos + 46 + name_len]).to_string();
        pos += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue; // directory
        }
        // Local header repeats name/extra lengths.
        if local_off + 30 > bytes.len() || &bytes[local_off..local_off + 4] != b"PK\x03\x04" {
            continue;
        }
        let l_name_len = u16::from_le_bytes([bytes[local_off + 26], bytes[local_off + 27]]) as usize;
        let l_extra_len = u16::from_le_bytes([bytes[local_off + 28], bytes[local_off + 29]]) as usize;
        let data_start = local_off + 30 + l_name_len + l_extra_len;
        if data_start + comp_size > bytes.len() {
            return Err("فشرده‌سازی zip ناقص است".into());
        }
        let raw = &bytes[data_start..data_start + comp_size];
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut dec = DeflateDecoder::new(raw);
                let mut out = Vec::new();
                dec.read_to_end(&mut out).map_err(|e| format!("انفراش deflate شکست خورد: {e}"))?;
                out
            },
            other => return Err(format!("روش فشرده‌سازی {other} پشتیبانی نمی‌شود")),
        };
        if data.len() > 64 * 1024 * 1024 {
            return Err("ورودی zip بیش‌ازحد بزرگ است".into());
        }
        out.push(ZipEntry { name, data });
    }
    Ok(out)
}

fn entry<'a>(entries: &'a [ZipEntry], name: &str) -> Option<&'a [u8]> {
    entries.iter().find(|e| e.name == name).map(|e| e.data.as_slice())
}

fn xml_tags(data: &[u8], tag: &str) -> Vec<String> {
    let text = String::from_utf8_lossy(data).to_string();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find(&open) {
        let after_open = &rest[i + open.len()..];
        let content_start = match after_open.find('>') {
            Some(j) => j + 1,
            None => break,
        };
        let content_slice = &after_open[content_start..];
        let after_content = match content_slice.find(&close) {
            Some(j) => &content_slice[..j],
            None => break,
        };
        let cleaned = unescape_xml(after_content);
        if !cleaned.trim().is_empty() {
            out.push(cleaned);
        }
        rest = &rest[i + open.len() + content_start + after_content.len() + close.len()..];
    }
    out
}

fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// docx: visible paragraph text from word/document.xml.
pub fn docx_text(bytes: &[u8]) -> Result<String, String> {
    let entries = zip_entries(bytes)?;
    let doc = entry(&entries, "word/document.xml")
        .ok_or("word/document.xml در فایل پیدا نشد (docx معتبر نیست)")?;
    let mut parts: Vec<String> = Vec::new();
    let text = String::from_utf8_lossy(doc).to_string();
    for para in text.split("</w:p>") {
        let mut line = String::new();
        for t in xml_tags(para.as_bytes(), "w:t") {
            line.push_str(&t);
        }
        if !line.trim().is_empty() {
            parts.push(line.trim().to_string());
        }
    }
    if parts.is_empty() {
        return Err("متنی در این docx پیدا نشد (احتمالاً تصویر/اسکن است)".into());
    }
    Ok(parts.join("\n"))
}

/// xlsx: shared strings + inline sheet cell values (best effort, capped).
pub fn xlsx_text(bytes: &[u8]) -> Result<String, String> {
    let entries = zip_entries(bytes)?;
    let mut parts: Vec<String> = Vec::new();
    if let Some(shared) = entry(&entries, "xl/sharedStrings.xml") {
        parts.extend(xml_tags(shared, "t").into_iter().take(4000));
    }
    for e in &entries {
        if e.name.starts_with("xl/worksheets/sheet") && e.name.ends_with(".xml") {
            let vals = xml_tags(&e.data, "v");
            if !vals.is_empty() {
                parts.extend(vals.into_iter().take(4000));
            }
        }
    }
    if parts.is_empty() {
        return Err("متنی در این xlsx پیدا نشد".into());
    }
    Ok(parts.join("\n"))
}

fn find_token(bytes: &[u8], from: usize, tok: &[u8]) -> Option<usize> {
    let n = bytes.len();
    if from >= n {
        return None;
    }
    let mut i = from;
    while i + tok.len() <= n {
        if &bytes[i..i + tok.len()] == tok {
            let prev_ok = i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'/' && bytes[i - 1] != b'%' && bytes[i - 1] != b'<');
            let next = i + tok.len();
            let next_ok = next >= n || !bytes[next].is_ascii_alphanumeric();
            if prev_ok && next_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Naive PDF text scan: literal strings used by text-showing operators
/// (Tj / TJ / ' / ") inside BT...ET blocks. Works for simple, uncompressed
/// text PDFs; compressed content streams are not decoded (the tool says so).
pub fn pdf_text(bytes: &[u8]) -> Result<String, String> {
    let len = bytes.len();
    let mut out = String::new();
    let mut i = 0usize;
    'outer: while i + 2 <= len && out.len() <= 200_000 {
        let Some(bt) = find_token(bytes, i, b"BT") else { break };
        let et = find_token(bytes, bt + 2, b"ET");
        let block_end = et.unwrap_or(len);
        let block = &bytes[bt + 2..block_end];
        let mut j = 0usize;
        while j + 1 < block.len() && out.len() <= 200_000 {
            if block[j] == b'(' {
                j += 1;
                let mut s = String::new();
                let mut parens = 1i32;
                while j < block.len() {
                    let c = block[j];
                    if c == b'\\' && j + 1 < block.len() {
                        let n = block[j + 1];
                        match n {
                            b'n' => s.push('\n'),
                            b'r' => s.push('\r'),
                            b'(' => s.push('('),
                            b')' => s.push(')'),
                            b'\\' => s.push('\\'),
                            b't' => s.push('\t'),
                            b'b' => s.push('\u{8}'),
                            b'f' => s.push('\u{c}'),
                            _ => {
                                if (b'0'..=b'7').contains(&n) {
                                    let mut k = j + 1;
                                    let mut oct = String::new();
                                    while k < block.len() && k <= j + 3 && (b'0'..=b'7').contains(&block[k]) {
                                        oct.push(block[k] as char);
                                        k += 1;
                                    }
                                    if let Ok(v) = u32::from_str_radix(&oct, 8) {
                                        if (0..=0x7F).contains(&v) {
                                            s.push(v as u8 as char);
                                        }
                                    }
                                    j = k;
                                    continue;
                                }
                                s.push(n as char);
                            }
                        }
                        j += 2;
                        continue;
                    }
                    if c == b'(' {
                        parens += 1;
                        s.push('(');
                    } else if c == b')' {
                        parens -= 1;
                        if parens == 0 {
                            j += 1;
                            break;
                        }
                        s.push(')');
                    } else {
                        s.push(c as char);
                    }
                    j += 1;
                }
                let mut k = j;
                while k < block.len() && block[k].is_ascii_whitespace() {
                    k += 1;
                }
                let is_tj = (k + 2 <= block.len() && (&block[k..k + 2] == b"Tj" || &block[k..k + 2] == b"TJ"))
                    || (k < block.len() && (block[k] == b'\'' || block[k] == b'"'));
                if is_tj && !s.trim().is_empty() {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(&s);
                }
                continue;
            }
            j += 1;
        }
        i = block_end.max(bt + 3);
        if i >= len {
            break 'outer;
        }
    }
    if out.trim().is_empty() {
        return Err("متنی قابل‌بازخوانی در این PDF پیدا نشد (جریان فشرده یا اسکن است؛ از دیدن تصویر با مدلِ ویژن‌دار استفاده کن)".into());
    }
    Ok(out.chars().take(200_000).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for i in 0..256u32 {
            let mut c = i;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            }
            table[i as usize] = c;
        }
        let mut c = 0xFFFFFFFFu32;
        for &b in data {
            c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        c ^ 0xFFFFFFFF
    }

    /// Hand-build a stored (uncompressed) zip with one entry.
    fn stored_zip(name: &str, content: &[u8]) -> Vec<u8> {
        let name_b = name.as_bytes();
        let crc = crc32(content);
        let sz = content.len() as u32;
        let mut v = Vec::new();
        let u16 = |x: u16| x.to_le_bytes();
        let u32 = |x: u32| x.to_le_bytes();
        // Local file header.
        v.extend_from_slice(b"PK\x03\x04");
        v.extend_from_slice(&u16(20)); // version needed
        v.extend_from_slice(&u16(0)); // flags
        v.extend_from_slice(&u16(0)); // method: stored
        v.extend_from_slice(&u16(0)); // mod time
        v.extend_from_slice(&u16(0x21)); // mod date (non-zero-ish)
        v.extend_from_slice(&u32(crc));
        v.extend_from_slice(&u32(sz));
        v.extend_from_slice(&u32(sz));
        v.extend_from_slice(&u16(name_b.len() as u16));
        v.extend_from_slice(&u16(0)); // extra len
        v.extend_from_slice(name_b);
        v.extend_from_slice(content);
        let cd_start = v.len() as u32;
        // Central directory.
        v.extend_from_slice(b"PK\x01\x02");
        v.extend_from_slice(&u16(20)); // version made by
        v.extend_from_slice(&u16(20)); // version needed
        v.extend_from_slice(&u16(0)); // flags
        v.extend_from_slice(&u16(0)); // method: stored
        v.extend_from_slice(&u16(0)); // mod time
        v.extend_from_slice(&u16(0x21)); // mod date
        v.extend_from_slice(&u32(crc));
        v.extend_from_slice(&u32(sz)); // compressed
        v.extend_from_slice(&u32(sz)); // uncompressed
        v.extend_from_slice(&u16(name_b.len() as u16));
        v.extend_from_slice(&u16(0)); // extra len
        v.extend_from_slice(&u16(0)); // comment len
        v.extend_from_slice(&u16(0)); // disk number
        v.extend_from_slice(&u16(0)); // internal attrs
        v.extend_from_slice(&u32(0)); // external attrs
        v.extend_from_slice(&u32(0)); // local header offset
        v.extend_from_slice(name_b);
        let cd_size = (v.len() - cd_start as usize) as u32;
        // End of central directory.
        v.extend_from_slice(b"PK\x05\x06");
        v.extend_from_slice(&u16(0)); // disk number
        v.extend_from_slice(&u16(0)); // disk with cd
        v.extend_from_slice(&u16(1)); // entries on disk
        v.extend_from_slice(&u16(1)); // total entries
        v.extend_from_slice(&u32(cd_size));
        v.extend_from_slice(&u32(cd_start));
        v.extend_from_slice(&u16(0)); // comment len
        v
    }

    #[test]
    fn zip_roundtrip_stored() {
        let z = stored_zip("word/document.xml", b"<hello/>");
        let entries = zip_entries(&z).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "word/document.xml");
        assert_eq!(entries[0].data, b"<hello/>");
    }

    #[test]
    fn docx_text_extract() {
        let xml = "<?xml version=\"1.0\"?><w:document><w:body><w:p><w:r><w:t>سلام</w:t></w:r><w:r><w:t>دنیا</w:t></w:r></w:p><w:p><w:r><w:t>خط دوم</w:t></w:r></w:p></w:body></w:document>".as_bytes();
        let z = stored_zip("word/document.xml", xml);
        let t = docx_text(&z).unwrap();
        assert!(t.contains("سلامدنیا"));
        assert!(t.contains("خط دوم"));
    }

    #[test]
    fn pdf_text_scan() {
        let pdf = b"%PDF-1.4\n1 0 obj\n<< /Length 60 >>\nstream\nBT\n/F1 12 Tf\n(Hello) Tj\n(Wor(l)d) Tj\nET\nendstream\nendobj\ntrailer\n%%EOF";
        let t = pdf_text(pdf).unwrap();
        assert!(t.contains("Hello"));
        assert!(t.contains("Wor(l)d"));
    }

    #[test]
    fn bad_zip_rejected() {
        assert!(zip_entries(b"PK\x05\x06").is_err());
        assert!(zip_entries(b"not a zip at all").is_err());
    }
}
