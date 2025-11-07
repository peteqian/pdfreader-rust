// Cross-reference (xref) table parser.
// xref is a cross-reference table, essentially a table of contents for PDF data.
// It maps object numbers to their byte positions in the file, allowing quick lookup without reading sequentially.
// Example: Object 1 is at byte 8433, Object 52 is at byte 192593, etc.

use crate::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct XrefEntry {
    pub offset: u64,
    pub generation: u16,
    pub in_use: bool,
}

pub struct XrefTable {
    pub entries: HashMap<u32, XrefEntry>,
}

impl XrefTable {
    pub fn new() -> Self {
        XrefTable {
            entries: HashMap::new(),
        }
    }

    pub fn parse(data: &[u8], xref_offset: usize) -> Result<(Self, HashMap<String, String>)> {
        if xref_offset >= data.len() {
            return Err(crate::PdfError::InvalidXref);
        }

        let xref_section = &data[xref_offset..];

        // Find "xref" keyword
        let xref_pos = xref_section.iter()
            .position(|&b| b == b'x')
            .and_then(|pos| {
                if xref_section[pos..].starts_with(b"xref") {
                    Some(pos)
                } else {
                    None
                }
            })
            .ok_or(crate::PdfError::InvalidXref)?;

        let mut entries = HashMap::new();
        let mut pos = xref_pos + 4;

        // Skip whitespace after "xref"
        while pos < xref_section.len() && (xref_section[pos] as char).is_whitespace() {
            pos += 1;
        }

        // Parse xref subsections
        while pos < xref_section.len() {
            // Check if we hit "trailer"
            if xref_section[pos..].starts_with(b"trailer") {
                break;
            }

            // Read starting object number and count
            let line_end = xref_section[pos..].iter()
                .position(|&b| b == b'\n' || b == b'\r')
                .unwrap_or(xref_section.len() - pos);

            let line = String::from_utf8_lossy(&xref_section[pos..pos + line_end]);
            let parts: Vec<&str> = line.split_whitespace().collect();

            if parts.len() >= 2 {
                if let (Ok(start), Ok(count)) = (parts[0].parse::<u32>(), parts[1].parse::<usize>()) {
                    pos += line_end + 1;

                    // Read entries
                    for i in 0..count {
                        // Skip whitespace
                        while pos < xref_section.len() && (xref_section[pos] as char).is_whitespace() {
                            pos += 1;
                        }

                        if pos + 20 > xref_section.len() {
                            break;
                        }

                        let entry_line = String::from_utf8_lossy(&xref_section[pos..pos + 20]);
                        let entry_parts: Vec<&str> = entry_line.split_whitespace().collect();

                        if entry_parts.len() >= 3 {
                            if let (Ok(offset), Ok(generation_num)) = (
                                entry_parts[0].parse::<u64>(),
                                entry_parts[1].parse::<u16>()
                            ) {
                                let in_use = entry_parts[2] == "n";
                                entries.insert(start + i as u32, XrefEntry { offset, generation: generation_num, in_use });
                            }
                        }

                        // Move to next line
                        pos += xref_section[pos..].iter()
                            .position(|&b| b == b'\n' || b == b'\r')
                            .unwrap_or(xref_section.len() - pos);
                        pos += 1;
                    }
                }
            } else {
                pos += line_end + 1;
            }
        }

        // Parse trailer
        let trailer = parse_trailer(&xref_section[pos..])?;

        let table = XrefTable { entries };

        Ok((table, trailer))
    }
}

fn parse_trailer(data: &[u8]) -> Result<HashMap<String, String>> {
    let mut trailer = HashMap::new();

    // Find "trailer" keyword
    if !data.starts_with(b"trailer") {
        return Err(crate::PdfError::MissingTrailer);
    }

    // Find the dictionary content (between << and >>)
    if let Some(dict_start) = data.iter().position(|&b| b == b'<').and_then(|pos| {
        if data[pos..].starts_with(b"<<") {
            Some(pos + 2)
        } else {
            None
        }
    }) {
        if let Some(dict_end) = data[dict_start..].iter().position(|&b| b == b'>').and_then(|pos| {
            if data[dict_start + pos..].starts_with(b">>") {
                Some(dict_start + pos)
            } else {
                None
            }
        }) {
            let dict_content = String::from_utf8_lossy(&data[dict_start..dict_end]);

            // Parse simple key-value pairs
            let parts: Vec<&str> = dict_content.split('/').collect();
            for i in 1..parts.len() {
                let kv: Vec<&str> = parts[i].splitn(2, |c: char| c.is_whitespace()).collect();
                if kv.len() >= 2 {
                    let key = kv[0].to_string();
                    let value = kv[1].trim().to_string();
                    trailer.insert(key, value);
                }
            }
        }
    }

    Ok(trailer)
}
