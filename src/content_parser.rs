use std::collections::HashMap;

use crate::fonts::{self, FontInfo};
use crate::lexer::{Lexer, Token};
use crate::objects::Object;

/// Represents a value encountered in a PDF content stream.
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum ContentValue {
    Number(f64),
    String(Vec<u8>),
    Array(Vec<ContentValue>),
    Name(String),
}

/// Tracks the current state while parsing a content stream.
///
/// This includes the current font and available resources for glyph mapping.
#[allow(dead_code)]
struct ContentState {
    current_font: Option<String>,                        // Font name like "/F1"
    resources: Option<HashMap<String, Object>>,         // /Resources from page
    fonts: HashMap<String, FontInfo>,                   // Extracted fonts from resources
    current_font_info: Option<Box<FontInfo>>,           // Current font info if available
}

impl ContentState {
    #[allow(dead_code)]
    fn new() -> Self {
        ContentState {
            current_font: None,
            resources: None,
            fonts: HashMap::new(),
            current_font_info: None,
        }
    }

    fn with_resources(resources: Option<HashMap<String, Object>>, objects: &HashMap<u32, Object>) -> Self {
        // Extract fonts from resources if available (Phase 2)
        let fonts = resources
            .as_ref()
            .map(|res| fonts::extract_fonts_with_objects(res, objects))
            .unwrap_or_default();

        // Log font information for debugging
        fonts::log_font_info(&fonts);

        ContentState {
            current_font: None,
            resources,
            fonts,
            current_font_info: None,
        }
    }

    /// Updates the current font when a Tf operator is encountered.
    fn set_font(&mut self, font_name: String) {
        self.current_font = Some(font_name.clone());
        self.current_font_info = self.fonts.get(&font_name).map(|f| Box::new(f.clone()));
    }
}

/// Parses a decoded content stream and extracts text.
///
/// Iterates through tokens, collects operands, and dispatches to operator handlers.
/// The optional resources parameter should contain the /Resources dictionary from the page,
/// which is needed for font lookup and glyph-to-Unicode mapping (Phase 2).
/// The objects parameter provides the full object map for resolving font references.
///
/// Returns a string with extracted text content.
pub fn parse_content_stream(
    data: &[u8],
    resources: Option<HashMap<String, Object>>,
    objects: &HashMap<u32, Object>,
) -> Result<String, String> {
    let mut lexer = Lexer::new(data.to_vec());
    let mut operands: Vec<ContentValue> = Vec::new();
    let mut text = String::new();
    let mut state = ContentState::with_resources(resources, objects);

    loop {
        match lexer.next_token() {
            Ok(Token::Eof) => break,
            Ok(Token::Keyword(op)) => {
                handle_operator(&op, &mut operands, &mut text, &mut state);
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

/// Parses an array from the content stream.
///
/// Handles nested arrays and stops at `]` or EOF.
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

/// Dispatches text operators to appropriate handlers.
///
/// Supported operators:
/// - `Tf`: Set text font and size (track font for Phase 2 glyph mapping)
/// - `Tj`: Show string (single operand)
/// - `TJ`: Show string array (array of strings and numbers)
/// - `Td`, `TD`, `T*`: Text positioning (insert newline)
/// - `'`, `"`: Text showing operators with operands
/// - `BT`: Begin text object
/// - `ET`: End text object (insert newline)
/// - Other operators: ignored
fn handle_operator(
    op: &str,
    operands: &mut Vec<ContentValue>,
    text: &mut String,
    state: &mut ContentState,
) {
    match op {
        "Tf" => {
            // Tf operator: Set current font
            // Syntax: <font_name> <size> Tf
            // We track the font name for Phase 2 glyph mapping
            if operands.len() >= 2 {
                // Font name is second-to-last operand (before size)
                if let Some(ContentValue::Name(font_name)) = operands.get(operands.len() - 2) {
                    if let Some(ContentValue::Number(size)) = operands.get(operands.len() - 1) {
                        eprintln!("[Content] Tf operator: font={} size={}", font_name, size);
                    }
                    state.set_font(font_name.clone());
                }
            }
        }
        "Tj" => {
            if let Some(ContentValue::String(bytes)) = operands.pop() {
                append_text(text, &bytes, state);
            }
        }
        "TJ" => {
            if let Some(ContentValue::Array(items)) = operands.pop() {
                for item in items {
                    if let ContentValue::String(bytes) = item {
                        append_text(text, &bytes, state);
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
                append_text(text, &bytes, state);
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

/// Appends bytes to the text output.
///
/// Attempts to use font encoding and glyph mapping if available (Phase 2).
/// Falls back to UTF-8 lossy conversion if font information is not available.
///
/// NOTE: This directly appends text without adding automatic spaces. The PDF
/// content stream should contain actual space characters where needed, or use
/// positioning operators (Td, Tm, etc.) to control spacing. Automatic spacing
/// can break words that are rendered character-by-character with positioning ops.
fn append_text(output: &mut String, bytes: &[u8], state: &ContentState) {
    // TODO (Phase 2): Implement font-aware glyph mapping using state.current_font_info
    // For now, use basic glyph-to-unicode conversion
    for byte in bytes {
        let char_str = fonts::glyph_to_unicode(*byte, state.current_font_info.as_deref());
        output.push_str(char_str.trim_matches('\0'));
    }
}
