use std::env;
use std::fs;
use pdfreader::xref::XrefTable;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <pdf-file>", args[0]);
        std::process::exit(1);
    }

    let pdf_path = &args[1];

    println!("PDF Math Recognition Library");
    println!("Phase 1: Reading a PDF\n");
    println!("File: {}", pdf_path);

    match read_pdf(pdf_path) {
        Ok((header, size)) => {
            println!("Size: {} bytes", size);
            println!("Header: {}", header);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn read_pdf(path: &str) -> Result<(String, usize), String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let size = bytes.len();
    let header = parse_header(&bytes)?;
    parse_xref_and_trailer(&bytes)?;
    Ok((header, size))
}

fn parse_header(bytes: &[u8]) -> Result<String, String> {
    let header_end = bytes
        .windows(4)
        .position(|w| w == b"\r\n" || w == b"\n\n" || w[0] == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(10);

    let header_bytes = &bytes[0..header_end.min(bytes.len())];
    String::from_utf8_lossy(header_bytes)
        .trim()
        .to_string()
        .parse::<String>()
        .map_err(|_| "Invalid PDF header".to_string())
}

fn parse_xref_and_trailer(bytes: &[u8]) -> Result<(), String> {
    let search_from = bytes.len().saturating_sub(1024);
    let end_section = &bytes[search_from..];

    if let Some(pos) = find_keyword(end_section, b"startxref") {
        let xref_offset_start = pos + 9;
        let xref_bytes = &end_section[xref_offset_start..];

        let offset_start = xref_bytes.iter().position(|&b| b >= b'0' && b <= b'9').unwrap_or(0);
        let offset_end = offset_start + xref_bytes[offset_start..]
            .iter()
            .position(|&b| b < b'0' || b > b'9')
            .unwrap_or(0);

        let offset_str = String::from_utf8_lossy(&xref_bytes[offset_start..offset_end]);
        if let Ok(xref_offset) = offset_str.parse::<usize>() {
            println!("Found startxref at offset: {} bytes", xref_offset);

            // Parse xref table and trailer
            match XrefTable::parse(bytes, xref_offset) {
                Ok((table, trailer)) => {
                    println!("✓ xref table parsed\n");
                    println!("Xref Entries: {}", table.entries.len());
                    for (obj_num, entry) in table.entries.iter() {
                        let status = if entry.in_use { "in-use" } else { "free" };
                        println!("  Object {}: offset={}, gen={}, {}", obj_num, entry.offset, entry.generation, status);
                    }
                    println!("\nTrailer:");
                    for (key, value) in trailer.iter() {
                        println!("  {}: {}", key, value);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to parse xref: {:?}", e);
                }
            }
        }
    }

    Ok(())
}

fn find_keyword(bytes: &[u8], keyword: &[u8]) -> Option<usize> {
    bytes.windows(keyword.len()).position(|w| w == keyword)
}

