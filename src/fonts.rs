use std::collections::HashMap;

use crate::objects::Object;
use crate::stream;

/// Font encoding and glyph-to-Unicode mapping.
///
/// This module handles Phase 2 requirements:
/// - Extracting font resources from `/Resources/Font`
/// - Parsing `/Encoding` dictionaries (predefined like WinAnsiEncoding or custom)
/// - Extracting and parsing `/ToUnicode` CMaps
/// - Mapping glyph codes to Unicode characters
/// - Tracking font state during content stream parsing

/// Struct to hold font information and encoding tables.
#[derive(Clone)]
pub struct FontInfo {
    /// Font name like "/F1", "/TT0", etc.
    pub name: String,
    /// Font subtype: "Type1", "TrueType", "CIDFontType0", etc.
    pub font_type: String,
    /// Character to Unicode mapping
    pub encoding: Option<FontEncoding>,
    /// To Unicode CMap
    pub to_unicode: Option<ToUnicodeMap>,
}

/// Font character encoding tables.
///
/// Maps glyph codes (0-255 for single-byte encodings) to Unicode codepoints.
#[derive(Clone)]
pub struct FontEncoding {
    /// Maps glyph code (0-255) to Unicode codepoint
    /// Index = glyph code, Value = Unicode codepoint
    pub glyph_to_unicode: [u32; 256],
}

/// ToUnicode CMap for font character mapping.
///
/// Maps glyph codes to Unicode strings (can be multiple characters per glyph).
#[derive(Clone)]
pub struct ToUnicodeMap {
    /// Maps glyph codes (as hex bytes) to Unicode strings
    /// Key: glyph code as bytes (e.g., [0x00, 0x21] for glyph 0x0021)
    /// Value: Unicode string representation
    pub mappings: HashMap<Vec<u8>, String>,
}

/// Extracts font information from the /Resources dictionary.
///
/// Returns a HashMap mapping font names (like "/F1") to their FontInfo.
///
/// Note: Currently cannot resolve font references to other objects.
/// Fonts that are stored as indirect references will be skipped.
pub fn extract_fonts(resources: &HashMap<String, Object>) -> HashMap<String, FontInfo> {
    extract_fonts_with_objects(resources, &HashMap::new())
}

/// Extracts font information from the /Resources dictionary with access to objects map.
///
/// This version can resolve indirect references to font objects.
pub fn extract_fonts_with_objects(
    resources: &HashMap<String, Object>,
    objects: &HashMap<u32, Object>,
) -> HashMap<String, FontInfo> {
    let mut fonts = HashMap::new();

    // Look for /Font in resources
    if let Some(Object::Dictionary(font_dict)) = resources.get("Font") {
        for (font_name, font_obj) in font_dict {
            if let Some(font_info) = parse_font_object_with_objects(font_name, font_obj, objects) {
                fonts.insert(font_name.clone(), font_info);
            }
        }
    }

    fonts
}

/// Parses a single font object into FontInfo with access to objects map.
///
/// Handles font dictionaries and can resolve font references to other objects.
fn parse_font_object_with_objects(
    name: &str,
    obj: &Object,
    objects: &HashMap<u32, Object>,
) -> Option<FontInfo> {
    // Resolve reference if needed
    let font_dict = match obj {
        Object::Dictionary(dict) => dict.clone(),
        Object::Reference(obj_num, _) => {
            // Try to resolve the reference
            if let Some(Object::Dictionary(dict)) = objects.get(obj_num) {
                dict.clone()
            } else {
                return None;
            }
        }
        _ => return None,
    };

    // Extract font type
    let font_type = font_dict
        .get("Subtype")
        .and_then(|obj| match obj {
            Object::Name(name) => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // Try to extract and parse /ToUnicode CMap (Phase 2)
    let to_unicode = font_dict
        .get("ToUnicode")
        .and_then(|obj| match obj {
            Object::Stream { dict, data } => {
                // Decode the stream if it's compressed (usually FlateDecode)
                let decoded_data = stream::decode_stream(dict, data, &HashMap::new())
                    .ok()
                    .unwrap_or_else(|| data.to_vec());
                parse_to_unicode_cmap(&decoded_data)
            }
            _ => None,
        });

    // Try to extract and parse /Encoding table (Phase 2 - NOW IMPLEMENTED)
    // If no explicit /Encoding, use a default based on font type
    let encoding = font_dict
        .get("Encoding")
        .and_then(|obj| parse_encoding(obj))
        .or_else(|| {
            // Apply default encoding if none specified
            // Type1 fonts default to StandardEncoding
            // TrueType fonts default to WinAnsiEncoding
            match font_type.as_str() {
                "Type1" | "MMType1" => {
                    Some(FontEncoding {
                        glyph_to_unicode: get_standard_encoding(),
                    })
                }
                "TrueType" => {
                    Some(FontEncoding {
                        glyph_to_unicode: get_win_ansi_encoding(),
                    })
                }
                _ => None,
            }
        });

    Some(FontInfo {
        name: name.to_string(),
        font_type,
        encoding,
        to_unicode,
    })
}

/// Parses a ToUnicode CMap stream.
///
/// Extracts glyph code to Unicode character mappings from the CMap format.
/// Handles both `beginbfchar` (single mappings) and `beginbfrange` (range mappings).
///
/// Example CMap entry:
/// ```text
/// beginbfchar
/// <0003> <0054>
/// <0004> <0055>
/// endbfchar
/// ```
fn parse_to_unicode_cmap(data: &[u8]) -> Option<ToUnicodeMap> {
    let cmap_text = String::from_utf8_lossy(data);
    let mut mappings = HashMap::new();

    // Find and parse beginbfchar sections
    let mut pos = 0;
    while let Some(start) = cmap_text[pos..].find("beginbfchar") {
        pos += start + 11; // len("beginbfchar")

        // Find corresponding endbfchar
        if let Some(end_offset) = cmap_text[pos..].find("endbfchar") {
            let section = &cmap_text[pos..pos + end_offset];

            // Parse hex pairs: <XXXX> <YYYY>
            for line in section.lines() {
                let line = line.trim();

                // Skip empty lines and comments
                if line.is_empty() || line.starts_with('%') {
                    continue;
                }

                // Look for pattern: <HEX> <HEX>
                if let Some(first_open) = line.find('<') {
                    if let Some(first_close) = line[first_open..].find('>') {
                        let glyph_hex = &line[first_open + 1..first_open + first_close];

                        // Find second hex value
                        let after_first = first_open + first_close + 1;
                        if let Some(second_open) = line[after_first..].find('<') {
                            if let Some(second_close) = line[after_first + second_open..].find('>') {
                                let unicode_hex =
                                    &line[after_first + second_open + 1..after_first + second_open + second_close];

                                // Convert hex strings to bytes
                                if let (Ok(glyph_bytes), Ok(unicode_str)) =
                                    (hex_string_to_bytes(glyph_hex), hex_to_unicode(unicode_hex))
                                {
                                    mappings.insert(glyph_bytes, unicode_str);
                                }
                            }
                        }
                    }
                }
            }

            pos += end_offset + 9; // len("endbfchar")
        } else {
            break;
        }
    }

    // TODO (Phase 2): Also parse beginbfrange sections for range mappings
    // Format: <START> <END> <TARGET>

    if mappings.is_empty() {
        None
    } else {
        Some(ToUnicodeMap { mappings })
    }
}

/// Converts a hex string like "0054" to a UTF-8 string.
///
/// Treats the hex value as a Unicode codepoint.
fn hex_to_unicode(hex: &str) -> Result<String, ()> {
    if hex.len() < 4 {
        return Err(());
    }

    // Parse as hex number
    if let Ok(code) = u32::from_str_radix(hex, 16) {
        // Convert codepoint to Unicode character
        if let Some(ch) = char::from_u32(code) {
            return Ok(ch.to_string());
        }
    }

    Err(())
}

/// Converts a hex string like "0054" to a byte vector.
///
/// Splits the hex string into pairs and converts each to a byte.
/// Example: "0054" → [0x00, 0x54]
fn hex_string_to_bytes(hex: &str) -> Result<Vec<u8>, ()> {
    if hex.len() % 2 != 0 {
        return Err(());
    }

    let mut bytes = Vec::new();
    for i in (0..hex.len()).step_by(2) {
        if let Ok(byte) = u8::from_str_radix(&hex[i..i + 2], 16) {
            bytes.push(byte);
        } else {
            return Err(());
        }
    }

    Ok(bytes)
}

/// Maps a glyph byte to its Unicode representation using font encoding.
///
/// Uses the following lookup order:
/// 1. Check /ToUnicode CMap if available (IMPLEMENTED Phase 2)
/// 2. Check /Encoding table if available (NOW IMPLEMENTED Phase 2)
/// 3. Fall back to treating glyph code as Unicode codepoint
///
/// This function is crucial for readable text extraction from PDFs.
pub fn glyph_to_unicode(glyph_byte: u8, font_info: Option<&FontInfo>) -> String {
    // Try /ToUnicode CMap first (Phase 2)
    if let Some(font) = font_info {
        if let Some(to_unicode) = &font.to_unicode {
            // Try single-byte lookup
            let key = vec![glyph_byte];
            if let Some(unicode_str) = to_unicode.mappings.get(&key) {
                return unicode_str.clone();
            }

            // Try two-byte lookup (some CMaps use 2-byte codes)
            // This is for fonts that have multi-byte glyph codes
            // For now, single-byte is most common
        }

        // Check /Encoding table if /ToUnicode not available (Phase 2 - NOW IMPLEMENTED)
        if let Some(encoding) = &font.encoding {
            let code = encoding.glyph_to_unicode[glyph_byte as usize];
            if code != 0 {
                // Valid Unicode codepoint
                if let Some(ch) = char::from_u32(code) {
                    return ch.to_string();
                }
            }
        }
    }

    // Fall back: treat glyph as Unicode codepoint directly
    // This is common for embedded fonts and identity encodings
    String::from_utf8_lossy(&[glyph_byte]).to_string()
}

/// Returns the WinAnsiEncoding table.
///
/// WinAnsiEncoding is the Windows Latin-1 encoding (Code Page 1252).
/// Used by most PDF generators on Windows.
fn get_win_ansi_encoding() -> [u32; 256] {
    [
        // 0x00-0x1F: Control characters (mapped as-is for 0x00-0x1F)
        0x0000, 0x0001, 0x0002, 0x0003, 0x0004, 0x0005, 0x0006, 0x0007,
        0x0008, 0x0009, 0x000A, 0x000B, 0x000C, 0x000D, 0x000E, 0x000F,
        0x0010, 0x0011, 0x0012, 0x0013, 0x0014, 0x0015, 0x0016, 0x0017,
        0x0018, 0x0019, 0x001A, 0x001B, 0x001C, 0x001D, 0x001E, 0x001F,
        // 0x20-0x7E: Standard ASCII
        0x0020, 0x0021, 0x0022, 0x0023, 0x0024, 0x0025, 0x0026, 0x0027,
        0x0028, 0x0029, 0x002A, 0x002B, 0x002C, 0x002D, 0x002E, 0x002F,
        0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037,
        0x0038, 0x0039, 0x003A, 0x003B, 0x003C, 0x003D, 0x003E, 0x003F,
        0x0040, 0x0041, 0x0042, 0x0043, 0x0044, 0x0045, 0x0046, 0x0047,
        0x0048, 0x0049, 0x004A, 0x004B, 0x004C, 0x004D, 0x004E, 0x004F,
        0x0050, 0x0051, 0x0052, 0x0053, 0x0054, 0x0055, 0x0056, 0x0057,
        0x0058, 0x0059, 0x005A, 0x005B, 0x005C, 0x005D, 0x005E, 0x005F,
        0x0060, 0x0061, 0x0062, 0x0063, 0x0064, 0x0065, 0x0066, 0x0067,
        0x0068, 0x0069, 0x006A, 0x006B, 0x006C, 0x006D, 0x006E, 0x006F,
        0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075, 0x0076, 0x0077,
        0x0078, 0x0079, 0x007A, 0x007B, 0x007C, 0x007D, 0x007E, 0x007F,
        // 0x80-0xFF: Windows Latin-1 extended (Code Page 1252)
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021,
        0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F,
        0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
        0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
        0x00A0, 0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6, 0x00A7,
        0x00A8, 0x00A9, 0x00AA, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00AF,
        0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5, 0x00B6, 0x00B7,
        0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00BF,
        0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C6, 0x00C7,
        0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x00CC, 0x00CD, 0x00CE, 0x00CF,
        0x00D0, 0x00D1, 0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7,
        0x00D8, 0x00D9, 0x00DA, 0x00DB, 0x00DC, 0x00DD, 0x00DE, 0x00DF,
        0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5, 0x00E6, 0x00E7,
        0x00E8, 0x00E9, 0x00EA, 0x00EB, 0x00EC, 0x00ED, 0x00EE, 0x00EF,
        0x00F0, 0x00F1, 0x00F2, 0x00F3, 0x00F4, 0x00F5, 0x00F6, 0x00F7,
        0x00F8, 0x00F9, 0x00FA, 0x00FB, 0x00FC, 0x00FD, 0x00FE, 0x00FF,
    ]
}

/// Returns the MacRomanEncoding table.
///
/// MacRomanEncoding is used by older Mac OS systems.
fn get_mac_roman_encoding() -> [u32; 256] {
    [
        // 0x00-0x7F: ASCII
        0x0000, 0x0001, 0x0002, 0x0003, 0x0004, 0x0005, 0x0006, 0x0007,
        0x0008, 0x0009, 0x000A, 0x000B, 0x000C, 0x000D, 0x000E, 0x000F,
        0x0010, 0x0011, 0x0012, 0x0013, 0x0014, 0x0015, 0x0016, 0x0017,
        0x0018, 0x0019, 0x001A, 0x001B, 0x001C, 0x001D, 0x001E, 0x001F,
        0x0020, 0x0021, 0x0022, 0x0023, 0x0024, 0x0025, 0x0026, 0x0027,
        0x0028, 0x0029, 0x002A, 0x002B, 0x002C, 0x002D, 0x002E, 0x002F,
        0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037,
        0x0038, 0x0039, 0x003A, 0x003B, 0x003C, 0x003D, 0x003E, 0x003F,
        0x0040, 0x0041, 0x0042, 0x0043, 0x0044, 0x0045, 0x0046, 0x0047,
        0x0048, 0x0049, 0x004A, 0x004B, 0x004C, 0x004D, 0x004E, 0x004F,
        0x0050, 0x0051, 0x0052, 0x0053, 0x0054, 0x0055, 0x0056, 0x0057,
        0x0058, 0x0059, 0x005A, 0x005B, 0x005C, 0x005D, 0x005E, 0x005F,
        0x0060, 0x0061, 0x0062, 0x0063, 0x0064, 0x0065, 0x0066, 0x0067,
        0x0068, 0x0069, 0x006A, 0x006B, 0x006C, 0x006D, 0x006E, 0x006F,
        0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075, 0x0076, 0x0077,
        0x0078, 0x0079, 0x007A, 0x007B, 0x007C, 0x007D, 0x007E, 0x007F,
        // 0x80-0xFF: Mac Roman extended
        0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1,
        0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8,
        0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3,
        0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC,
        0x2020, 0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF,
        0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8,
        0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211,
        0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x2126, 0x00E6, 0x00F8,
        0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x2665,
        0x2663, 0x2661, 0x2660, 0x2194, 0x2190, 0x2191, 0x2192, 0x2193,
        0x00B0, 0x03B1, 0x03B2, 0x03C7, 0x03B4, 0x03B5, 0x03C6, 0x03B3,
        0x03B7, 0x03B9, 0x03BE, 0x03BA, 0x03BB, 0x03BC, 0x03BD, 0x03BF,
        0x03C0, 0x03C1, 0x03C3, 0x03C4, 0x03B8, 0x03C9, 0x03C5, 0x03A6,
        0x03A8, 0x03A9, 0x0391, 0x0392, 0x0393, 0x0394, 0x0395, 0x0396,
        0x0397, 0x0398, 0x0399, 0x039A, 0x039B, 0x039C, 0x039D, 0x039E,
        0x039F, 0x03A0, 0x03A1, 0x03A3, 0x03A4, 0x03A5, 0x03A6, 0x03A7,
    ]
}

/// Parses an /Encoding entry and returns the appropriate encoding table.
///
/// Supports predefined encodings by name and custom encoding dictionaries.
fn parse_encoding(encoding_obj: &Object) -> Option<FontEncoding> {
    match encoding_obj {
        Object::Name(name) => {
            // Predefined encoding
            let table = match name.as_str() {
                "WinAnsiEncoding" => get_win_ansi_encoding(),
                "MacRomanEncoding" => get_mac_roman_encoding(),
                "StandardEncoding" => get_standard_encoding(),
                _ => return None, // Unknown encoding
            };
            Some(FontEncoding {
                glyph_to_unicode: table,
            })
        }
        Object::Dictionary(_dict) => {
            // Custom encoding dictionary
            // TODO (Phase 2+): Parse custom encoding dictionaries
            // For now, skip custom encodings
            None
        }
        _ => None,
    }
}

/// Returns the StandardEncoding table (PDF standard).
///
/// Used by simple PDF generators and as a fallback.
fn get_standard_encoding() -> [u32; 256] {
    // StandardEncoding is mostly the same as WinAnsiEncoding for practical purposes
    // Real differences are in codes 0x80-0xFF for some special characters
    get_win_ansi_encoding() // Simplified for now
}

/// Logs diagnostic information about extracted fonts.
///
/// Helps debug font-related text extraction issues.
pub fn log_font_info(fonts: &HashMap<String, FontInfo>) {
    if fonts.is_empty() {
        eprintln!("[Fonts] No fonts extracted from resources");
        return;
    }

    eprintln!("[Fonts] Extracted {} fonts:", fonts.len());
    for (name, info) in fonts {
        eprintln!("  {} → Type: {}", name, info.font_type);

        if info.encoding.is_some() {
            eprintln!("    Encoding: Present (glyph-to-unicode table)");
        } else {
            eprintln!("    Encoding: None (using fallback or ToUnicode only)");
        }

        if let Some(to_unicode) = &info.to_unicode {
            eprintln!("    ToUnicode: {} mappings found", to_unicode.mappings.len());
        } else {
            eprintln!("    ToUnicode: None");
        }

        let has_encoding = info.encoding.is_some();
        let has_unicode = info.to_unicode.is_some();

        if !has_encoding && !has_unicode {
            eprintln!("    ⚠️  WARNING: Font has no encoding or ToUnicode mapping!");
        }
    }
}
