use std::collections::HashMap;

use crate::content_parser::parse_content_stream;
use crate::objects::Object;
use crate::stream::collect_content_streams;

/// Extracted text from a single PDF page.
#[derive(Debug, Clone)]
pub struct PageText {
    pub page_id: u32,
    pub text: String,
}

/// Extracts text from all pages in the PDF document.
///
/// Walks through all objects, identifies Page dictionaries,
/// collects their content streams, decodes them, parses them,
/// and extracts text content.
pub fn extract_text(objects: &HashMap<u32, Object>) -> Vec<PageText> {
    let mut pages = Vec::new();

    for (object_id, object) in objects {
        if let Some(dict) = object.as_dict() {
            if is_page(dict) {
                if let Some(text) = extract_page_stream_text(*object_id, dict, objects) {
                    pages.push(text);
                }
            }
        }
    }

    pages.sort_by_key(|page| page.page_id);
    pages
}

/// Checks if an object is a PDF Page dictionary.
fn is_page(dict: &HashMap<String, Object>) -> bool {
    matches!(dict.get("Type"), Some(Object::Name(name)) if name == "Page")
}

/// Extracts text from a single page's content streams.
///
/// Collects all content streams (handling single references, arrays,
/// and inline streams), decodes them, parses each, and combines the results.
fn extract_page_stream_text(
    page_id: u32,
    dict: &HashMap<String, Object>,
    objects: &HashMap<u32, Object>,
) -> Option<PageText> {
    let contents = dict.get("Contents")?;
    let streams = collect_content_streams(contents, objects);
    if streams.is_empty() {
        return None;
    }

    // Extract /Resources from page dictionary for font lookup (Phase 2)
    let resources = dict
        .get("Resources")
        .and_then(|obj| obj.as_dict())
        .cloned();

    // Diagnostic: show if page has resources
    if let Some(res) = &resources {
        eprintln!("[Page {}] Resources found:", page_id);
        for key in res.keys() {
            eprintln!("  - {}", key);
        }
    } else {
        eprintln!("[Page {}] No Resources dictionary found", page_id);
    }

    let mut page_text = String::new();
    for stream in streams {
        let content = parse_content_stream(&stream, resources.clone(), objects).unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }
        if !page_text.is_empty() {
            page_text.push('\n');
        }
        page_text.push_str(content.trim_end());
    }

    if page_text.is_empty() {
        None
    } else {
        Some(PageText {
            page_id,
            text: page_text,
        })
    }
}
