use std::collections::HashMap;

use crate::filters::{self, flate_decode};
use crate::objects::Object;

/// Collects all content streams from a Contents object.
///
/// Handles three cases:
/// - Single stream reference: `Object::Reference` → resolve → decode
/// - Array of references: iterate each and decode
/// - Inline stream: decode directly
///
/// Logs filter information for debugging.
pub fn collect_content_streams(
    contents: &Object,
    objects: &HashMap<u32, Object>,
) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();

    match contents {
        Object::Reference(_, _) => {
            if let Some(Object::Stream { dict, data }) = resolve_reference(objects, contents) {
                let filters = describe_filters(dict);
                eprintln!("[Stream] Filters: {}, Size: {} bytes", filters, data.len());
                match decode_stream(dict, data, objects) {
                    Ok(decoded) => {
                        eprintln!("[Stream] Decoded to: {} bytes", decoded.len());
                        streams.push(decoded);
                    }
                    Err(e) => {
                        eprintln!("[Stream] Failed to decode: {}", e);
                    }
                }
            }
        }
        Object::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                if let Some(Object::Stream { dict, data }) = resolve_reference(objects, item) {
                    let filters = describe_filters(dict);
                    eprintln!(
                        "[Stream {}] Filters: {}, Size: {} bytes",
                        idx,
                        filters,
                        data.len()
                    );
                    match decode_stream(dict, data, objects) {
                        Ok(decoded) => {
                            eprintln!("[Stream {}] Decoded to: {} bytes", idx, decoded.len());
                            streams.push(decoded);
                        }
                        Err(e) => {
                            eprintln!("[Stream {}] Failed to decode: {}", idx, e);
                        }
                    }
                }
            }
        }
        Object::Stream { dict, data } => {
            let filters = describe_filters(dict);
            eprintln!("[Stream] Filters: {}, Size: {} bytes", filters, data.len());
            match decode_stream(dict, data, objects) {
                Ok(decoded) => {
                    eprintln!("[Stream] Decoded to: {} bytes", decoded.len());
                    streams.push(decoded);
                }
                Err(e) => {
                    eprintln!("[Stream] Failed to decode: {}", e);
                }
            }
        }
        _ => {}
    }

    streams
}

/// Resolves indirect object references recursively.
///
/// Follows chains of references until reaching a non-reference object.
pub fn resolve_reference<'a>(
    objects: &'a HashMap<u32, Object>,
    object: &'a Object,
) -> Option<&'a Object> {
    let mut current = object;
    loop {
        match current {
            Object::Reference(obj_num, _) => {
                current = objects.get(obj_num)?;
            }
            _ => return Some(current),
        }
    }
}

/// Decodes stream data according to its `/Filter` directive.
///
/// Supports:
/// - No filter: returns data as-is
/// - `/Filter /FlateDecode`: zlib decompression
/// - `/Filter [...]`: filter chains applied in reverse order
/// - Indirect `/Length` references (e.g., `/Length 5 0 R`)
///
/// Implemented filters:
/// - FlateDecode: zlib compression
/// - ASCII85Decode: base-85 encoding
/// - ASCIIHexDecode: hex encoding
/// - LZWDecode: LZW compression
pub fn decode_stream(
    dict: &HashMap<String, Object>,
    data: &[u8],
    objects: &HashMap<u32, Object>,
) -> Result<Vec<u8>, String> {
    // Extract length - useful for validation and indirect reference resolution
    if let Some(stream_len) = extract_length(dict, objects) {
        eprintln!("[Stream] /Length: {} bytes (from {})",
            stream_len,
            if dict.contains_key("Length") {
                match dict.get("Length").unwrap() {
                    Object::Reference(_, _) => "indirect reference",
                    _ => "direct value"
                }
            } else {
                "unknown"
            }
        );
    }

    match dict.get("Filter") {
        None => Ok(data.to_vec()),
        Some(Object::Name(name)) => {
            // Single filter by name
            decode_with_filter(name, data)
        }
        Some(Object::Array(filters)) => {
            // Filter chain - apply in REVERSE order
            let mut result = data.to_vec();

            for filter_obj in filters.iter().rev() {
                match filter_obj {
                    Object::Name(filter_name) => {
                        result = decode_with_filter(filter_name, &result)?;
                    }
                    _ => return Err("Invalid filter specification in array".to_string()),
                }
            }

            Ok(result)
        }
        _ => Err("Unsupported stream filter specification".to_string()),
    }
}

/// Extracts the `/Length` value from a stream dictionary.
///
/// Handles both direct values (`Object::Number`) and indirect references (`Object::Reference`).
/// Returns `None` if `/Length` is not found or cannot be resolved.
///
/// # Examples
/// - Direct: `/Length 1234` → Some(1234)
/// - Indirect: `/Length 5 0 R` → resolves object 5, extracts number value
fn extract_length(
    dict: &HashMap<String, Object>,
    objects: &HashMap<u32, Object>,
) -> Option<u32> {
    match dict.get("Length") {
        None => None,
        Some(Object::Number(len)) => Some(*len as u32),
        Some(Object::Reference(obj_num, _)) => {
            // Resolve the reference to get the actual length value
            if let Some(Object::Number(len)) = objects.get(obj_num) {
                Some(*len as u32)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Applies a single filter decoder to data.
fn decode_with_filter(filter_name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    match filter_name {
        "FlateDecode" => flate_decode(data),
        "ASCII85Decode" | "ASCII85" => filters::ascii85_decode(data),
        "ASCIIHexDecode" | "AHx" => filters::ascii_hex_decode(data),
        "LZWDecode" => filters::lzw_decode(data),
        "CCITTFaxDecode" | "JBIG2Decode" | "DCTDecode" => {
            // Image compression - not text
            Err(format!("Image compression filter not supported: {}", filter_name))
        }
        other => Err(format!("Unsupported filter: {}", other)),
    }
}

/// Returns a human-readable description of the filters used in a stream.
///
/// Used for debug output.
pub fn describe_filters(dict: &HashMap<String, Object>) -> String {
    match dict.get("Filter") {
        None => "none".to_string(),
        Some(Object::Name(name)) => name.clone(),
        Some(Object::Array(filters)) => {
            let filter_names: Vec<String> = filters
                .iter()
                .filter_map(|f| match f {
                    Object::Name(name) => Some(name.clone()),
                    _ => None,
                })
                .collect();
            format!("[{}]", filter_names.join(", "))
        }
        _ => "unknown".to_string(),
    }
}
