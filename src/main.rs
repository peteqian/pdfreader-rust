mod exporters;
mod pipeline;

use std::collections::HashMap;
use std::env;
use std::fs;

use pdfreader::content::{extract_text, PageText};
use pdfreader::objects::{self, Object};
use pdfreader::xref::XrefTable;
use exporters::create_exporter;
use pipeline::{Pipeline, PipelineLog, finalize_log};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <pdf-file> [--format <format>]", args[0]);
        eprintln!();
        eprintln!("Formats: json, markdown, plaintext (default)");
        std::process::exit(1);
    }

    let pdf_path = &args[1];
    let export_format = parse_export_format(&args);
    println!("PDF Math Recognition Library");
    println!("Phase 1: Reading a PDF\n");
    println!("File: {}", pdf_path);

    let mut log = PipelineLog::new();
    log.info(format!("Starting pipeline for {}", pdf_path));

    let bytes = match fs::read(pdf_path) {
        Ok(bytes) => {
            log.info(format!("Read {} bytes", bytes.len()));
            bytes
        }
        Err(err) => {
            log.error(format!("Failed to read file: {}", err));
            finalize_log(pdf_path, &log);
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };

    let mut pipeline = Pipeline::new(&bytes, &mut log);
    pipeline.add_stage("header", |state, log| {
        let header = parse_header(state.bytes)?;
        log.info(format!("Header parsed: {}", header));
        state.doc.header = Some(header);
        Ok(())
    });
    pipeline.add_stage("xref", |state, log| {
        let (xref_table, trailer) = parse_xref_and_trailer(state.bytes)?;
        log.info(format!("Xref entries: {}", xref_table.entries.len()));
        state.doc.xref_table = Some(xref_table);
        state.doc.trailer = Some(trailer);
        Ok(())
    });
    pipeline.add_stage("objects", |state, log| {
        let xref = state
            .doc
            .xref_table
            .as_ref()
            .ok_or_else(|| "Xref must run before objects".to_string())?;
        let objects = objects::parse_objects(state.bytes, xref)
            .map_err(|e| format!("Failed to parse objects: {}", e))?;
        log.info(format!("Parsed {} objects", objects.len()));
        state.doc.objects = Some(objects);
        Ok(())
    });
    pipeline.add_stage("content", |state, log| {
        let objects = state
            .doc
            .objects
            .as_ref()
            .ok_or_else(|| "Objects must be parsed before content extraction".to_string())?;
        let pages = extract_text(objects);
        log.info(format!("Extracted text from {} pages", pages.len()));
        state.doc.pages = pages;
        Ok(())
    });

    let result = pipeline.run();

    match &result {
        Ok(_) => log.info("Pipeline completed successfully"),
        Err(err) => log.error(format!("Pipeline failed: {}", err)),
    }

    finalize_log(pdf_path, &log);

    match result {
        Ok(doc) => {
            println!("Size: {} bytes", doc.size);
            println!("Header: {}", doc.header);

            print_xref_summary(&doc.xref_table);
            print_trailer(&doc.trailer);
            print_object_summary(&doc.objects);

            // Export in the requested format
            let exporter = create_exporter(&export_format);
            let exported = exporter.export(&doc);

            // Write exported content to file
            let export_path = derive_export_path(pdf_path, &export_format);
            match fs::write(&export_path, &exported) {
                Ok(()) => println!("\n✓ Exported to {} (format: {})", export_path, export_format),
                Err(err) => eprintln!("Failed to write export file: {}", err),
            }
        }
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    }
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

fn parse_xref_and_trailer(bytes: &[u8]) -> Result<(XrefTable, HashMap<String, String>), String> {
    let search_from = bytes.len().saturating_sub(1024);
    let end_section = &bytes[search_from..];

    let pos =
        find_keyword(end_section, b"startxref").ok_or_else(|| "startxref not found".to_string())?;
    let xref_offset_start = pos + 9;
    let xref_bytes = &end_section[xref_offset_start..];

    let offset_start = xref_bytes
        .iter()
        .position(|&b| b >= b'0' && b <= b'9')
        .unwrap_or(0);
    let offset_end = offset_start
        + xref_bytes[offset_start..]
            .iter()
            .position(|&b| b < b'0' || b > b'9')
            .unwrap_or(0);

    let offset_str = String::from_utf8_lossy(&xref_bytes[offset_start..offset_end]);
    let xref_offset = offset_str
        .parse::<usize>()
        .map_err(|_| "Invalid startxref offset".to_string())?;

    XrefTable::parse(bytes, xref_offset).map_err(|e| format!("Failed to parse xref: {}", e))
}

fn find_keyword(bytes: &[u8], keyword: &[u8]) -> Option<usize> {
    bytes.windows(keyword.len()).position(|w| w == keyword)
}

fn print_xref_summary(table: &XrefTable) {
    println!("\n✓ xref table parsed");
    println!("Xref Entries: {}", table.entries.len());
    let mut entries: Vec<_> = table.entries.iter().collect();
    entries.sort_by_key(|(obj_num, _)| *obj_num);
    for (obj_num, entry) in entries.iter().take(10) {
        let status = if entry.in_use { "in-use" } else { "free" };
        println!(
            "  Object {}: offset={}, gen={}, {}",
            obj_num, entry.offset, entry.generation, status
        );
    }
    if table.entries.len() > 10 {
        println!("  ...");
    }
}

fn print_trailer(trailer: &HashMap<String, String>) {
    println!("\nTrailer:");
    let mut keys: Vec<_> = trailer.keys().collect();
    keys.sort();
    for key in keys {
        if let Some(value) = trailer.get(key) {
            println!("  {}: {}", key, value);
        }
    }
}

fn print_object_summary(objects: &HashMap<u32, Object>) {
    println!("\nObjects parsed: {}", objects.len());
    let mut keys: Vec<_> = objects.keys().cloned().collect();
    keys.sort_unstable();
    for key in keys.into_iter().take(8) {
        if let Some(object) = objects.get(&key) {
            println!("  Object {} -> {}", key, describe_object(object));
        }
    }
    if objects.len() > 8 {
        println!("  ...");
    }
}

fn print_page_text(pages: &[PageText]) {
    if pages.is_empty() {
        println!("\nExtracted text: (none)");
        return;
    }

    println!("\nExtracted text ({} page objects):", pages.len());
    for page in pages.iter().take(3) {
        println!("-- Page object {} --", page.page_id);
        println!("{}", page.text.trim());
        println!();
    }
    if pages.len() > 3 {
        println!("... ({} more pages)", pages.len() - 3);
    }
}

fn describe_object(object: &Object) -> String {
    match object {
        Object::Null => "Null".to_string(),
        Object::Boolean(v) => format!("Boolean {}", v),
        Object::Number(n) => format!("Number {}", n),
        Object::String(bytes) => format!("String ({} bytes)", bytes.len()),
        Object::Name(name) => format!("Name /{}", name),
        Object::Array(items) => format!("Array ({} items)", items.len()),
        Object::Dictionary(dict) => format!("Dictionary ({} entries)", dict.len()),
        Object::Stream { dict, data } => {
            let len = dict
                .get("Length")
                .and_then(|obj| obj.as_number())
                .unwrap_or(data.len() as f64);
            format!("Stream ({} entries, {} bytes)", dict.len(), len as usize)
        }
        Object::Reference(obj, generation) => format!("Reference {} {} R", obj, generation),
    }
}

/// Parses the --format flag from command line arguments.
///
/// Returns the export format name, defaulting to "plaintext" if not specified.
fn parse_export_format(args: &[String]) -> String {
    for i in 0..args.len() - 1 {
        if args[i] == "--format" {
            return args[i + 1].clone();
        }
    }
    "plaintext".to_string()
}

/// Derives the export file path from the PDF path and format.
///
/// Exports are written to the same directory as the input PDF.
///
/// Examples:
/// - "test-data/document.pdf" with "json" → "test-data/document.json"
/// - "test-data/document.pdf" with "markdown" → "test-data/document.md"
/// - "test-data/document.pdf" with "plaintext" → "test-data/document.txt"
fn derive_export_path(pdf_path: &str, format: &str) -> String {
    let path = std::path::Path::new(pdf_path);
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();

    let format_lower = format.to_lowercase();
    let extension = match format_lower.as_str() {
        "json" => "json",
        "markdown" | "md" => "md",
        "plaintext" | "text" | "txt" => "txt",
        other => other,
    };

    parent
        .join(format!("{}.{}", stem, extension))
        .to_string_lossy()
        .to_string()
}
