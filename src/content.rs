use std::collections::HashMap;
use std::os::raw::c_ulong;

use crate::lexer::{Lexer, Token};
use crate::objects::Object;

#[derive(Debug, Clone)]
pub struct PageText {
    pub page_id: u32,
    pub text: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ContentValue {
    Number(f64),
    String(Vec<u8>),
    Array(Vec<ContentValue>),
    Name(String),
}

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

fn is_page(dict: &HashMap<String, Object>) -> bool {
    matches!(dict.get("Type"), Some(Object::Name(name)) if name == "Page")
}

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

    let mut page_text = String::new();
    for stream in streams {
        let content = parse_content_stream(&stream).unwrap_or_default();
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

fn collect_content_streams(contents: &Object, objects: &HashMap<u32, Object>) -> Vec<Vec<u8>> {
    let mut streams = Vec::new();

    match contents {
        Object::Reference(_, _) => {
            if let Some(Object::Stream { dict, data }) = resolve_reference(objects, contents) {
                if let Ok(decoded) = decode_stream(dict, data) {
                    streams.push(decoded);
                }
            }
        }
        Object::Array(items) => {
            for item in items {
                if let Some(Object::Stream { dict, data }) = resolve_reference(objects, item) {
                    if let Ok(decoded) = decode_stream(dict, data) {
                        streams.push(decoded);
                    }
                }
            }
        }
        Object::Stream { dict, data } => {
            if let Ok(decoded) = decode_stream(dict, data) {
                streams.push(decoded);
            }
        }
        _ => {}
    }

    streams
}

fn resolve_reference<'a>(
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

fn decode_stream(dict: &HashMap<String, Object>, data: &[u8]) -> Result<Vec<u8>, String> {
    match dict.get("Filter") {
        None => Ok(data.to_vec()),
        Some(Object::Name(name)) if name == "FlateDecode" => flate_decode(data),
        Some(Object::Array(filters)) => {
            // Support simple single filter arrays (common in PDFs)
            if filters.len() == 1 {
                if let Object::Name(name) = &filters[0] {
                    if name == "FlateDecode" {
                        return flate_decode(data);
                    }
                }
            }
            Err("Unsupported stream filter".to_string())
        }
        _ => Err("Unsupported stream filter".to_string()),
    }
}

fn flate_decode(data: &[u8]) -> Result<Vec<u8>, String> {
    const Z_OK: i32 = 0;
    const Z_BUF_ERROR: i32 = -5;

    let mut capacity = data.len().saturating_mul(4).max(1024);
    loop {
        let mut buffer = vec![0u8; capacity];
        let mut dest_len = capacity as c_ulong;
        let src_len = data.len() as c_ulong;
        let status = unsafe {
            uncompress(
                buffer.as_mut_ptr(),
                &mut dest_len as *mut c_ulong,
                data.as_ptr(),
                src_len,
            )
        };

        match status {
            Z_OK => {
                buffer.truncate(dest_len as usize);
                return Ok(buffer);
            }
            Z_BUF_ERROR => {
                capacity *= 2;
                if capacity > 64 * 1024 * 1024 {
                    return Err("Stream too large to decode".to_string());
                }
            }
            code => {
                return Err(format!("zlib decode error: {}", code));
            }
        }
    }
}

#[link(name = "z")]
unsafe extern "C" {
    fn uncompress(
        dest: *mut u8,
        dest_len: *mut c_ulong,
        source: *const u8,
        source_len: c_ulong,
    ) -> i32;
}

fn parse_content_stream(data: &[u8]) -> Result<String, String> {
    let mut lexer = Lexer::new(data.to_vec());
    let mut operands: Vec<ContentValue> = Vec::new();
    let mut text = String::new();

    loop {
        match lexer.next_token() {
            Ok(Token::Eof) => break,
            Ok(Token::Keyword(op)) => {
                handle_operator(&op, &mut operands, &mut text);
                operands.clear();
            }
            Ok(Token::Number(n)) => operands.push(ContentValue::Number(n)),
            Ok(Token::String(bytes)) => operands.push(ContentValue::String(bytes)),
            Ok(Token::Name(name)) => operands.push(ContentValue::Name(name)),
            Ok(Token::LeftBracket) => {
                let array = parse_array(&mut lexer)?;
                operands.push(ContentValue::Array(array));
            }
            Ok(Token::RightBracket) => {}
            Ok(Token::LeftAngle) | Ok(Token::RightAngle) => {}
            Err(err) => return Err(format!("Content lexing error: {}", err)),
        }
    }

    Ok(text)
}

fn parse_array(lexer: &mut Lexer) -> Result<Vec<ContentValue>, String> {
    let mut items = Vec::new();

    loop {
        match lexer.next_token() {
            Ok(Token::RightBracket) => break,
            Ok(Token::Number(n)) => items.push(ContentValue::Number(n)),
            Ok(Token::String(bytes)) => items.push(ContentValue::String(bytes)),
            Ok(Token::Name(name)) => items.push(ContentValue::Name(name)),
            Ok(Token::LeftBracket) => {
                let nested = parse_array(lexer)?;
                items.push(ContentValue::Array(nested));
            }
            Ok(Token::Keyword(_)) | Ok(Token::LeftAngle) | Ok(Token::RightAngle) => {}
            Ok(Token::Eof) => break,
            Err(err) => return Err(format!("Content array parse error: {}", err)),
        }
    }

    Ok(items)
}

fn handle_operator(op: &str, operands: &mut Vec<ContentValue>, text: &mut String) {
    match op {
        "Tj" => {
            if let Some(ContentValue::String(bytes)) = operands.pop() {
                append_text(text, &bytes);
            }
        }
        "TJ" => {
            if let Some(ContentValue::Array(items)) = operands.pop() {
                for item in items {
                    if let ContentValue::String(bytes) = item {
                        append_text(text, &bytes);
                    }
                }
            }
        }
        "Td" | "TD" | "T*" => {
            if !text.ends_with('\n') {
                text.push('\n');
            }
        }
        "'" | "\"" => {
            // text-show operators that imply a newline
            if let Some(ContentValue::String(bytes)) = operands.pop() {
                append_text(text, &bytes);
                text.push('\n');
            }
        }
        "BT" => {}
        "ET" => {
            if !text.ends_with('\n') {
                text.push('\n');
            }
        }
        _ => {}
    }
}

fn append_text(output: &mut String, bytes: &[u8]) {
    if !output.is_empty() && !output.ends_with(' ') && !output.ends_with('\n') {
        output.push(' ');
    }
    let slice = String::from_utf8_lossy(bytes);
    output.push_str(slice.trim_matches('\0'));
}
