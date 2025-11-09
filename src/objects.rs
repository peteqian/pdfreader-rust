// PDF object types and representations.
// Represents the different data structures found in PDFs (numbers, strings, dictionaries, arrays, etc.).
// These are what we parse from tokens.

use std::collections::HashMap;

use crate::lexer::{Lexer, Token};
use crate::xref::XrefTable;
use crate::{PdfError, Result};

#[derive(Debug, Clone)]
pub enum Object {
    Null,
    Boolean(bool),
    Number(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Object>),
    Dictionary(HashMap<String, Object>),
    Stream {
        dict: HashMap<String, Object>,
        data: Vec<u8>,
    },
    Reference(u32, u16),
}

impl Object {
    pub fn as_dict(&self) -> Option<&HashMap<String, Object>> {
        match self {
            Object::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Object::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            Object::String(s) => Some(s),
            _ => None,
        }
    }
}

pub fn parse_objects(data: &[u8], xref: &XrefTable) -> Result<HashMap<u32, Object>> {
    let mut parsed = HashMap::new();
    let mut entries: Vec<_> = xref.entries.iter().collect();
    entries.sort_by_key(|(obj_num, _)| *obj_num);

    for (&object_number, entry) in entries {
        if !entry.in_use {
            continue;
        }
        let offset = entry.offset as usize;
        if offset >= data.len() {
            return Err(PdfError::InvalidObject);
        }

        let slice = &data[offset..];
        let (parsed_number, _, object) = parse_indirect_object(slice)?;
        if parsed_number == object_number {
            parsed.insert(object_number, object);
        }
    }

    Ok(parsed)
}

pub fn parse_indirect_object(bytes: &[u8]) -> Result<(u32, u16, Object)> {
    let mut lexer = Lexer::new(bytes.to_vec());
    let obj_number = match lexer.next_token()? {
        Token::Number(n) => to_u32(n).ok_or(PdfError::InvalidObject)?,
        _ => return Err(PdfError::InvalidObject),
    };

    let generation = match lexer.next_token()? {
        Token::Number(n) => to_u32(n).ok_or(PdfError::InvalidObject)? as u16,
        _ => return Err(PdfError::InvalidObject),
    };

    match lexer.next_token()? {
        Token::Keyword(keyword) if keyword == "obj" => {}
        _ => return Err(PdfError::InvalidObject),
    }

    let object = parse_object_value(&mut lexer)?;
    let object = maybe_parse_stream(object, &mut lexer)?;

    match lexer.next_token()? {
        Token::Keyword(keyword) if keyword == "endobj" => {}
        Token::Eof => {}
        _ => return Err(PdfError::InvalidObject),
    }

    Ok((obj_number, generation, object))
}

fn parse_object_value(lexer: &mut Lexer) -> Result<Object> {
    let token = lexer.next_token()?;
    parse_object_from_token(lexer, token)
}

fn parse_object_from_token(lexer: &mut Lexer, token: Token) -> Result<Object> {
    match token {
        Token::Number(num) => parse_number_or_reference(num, lexer),
        Token::Name(name) => Ok(Object::Name(name)),
        Token::String(bytes) => Ok(Object::String(bytes)),
        Token::LeftBracket => parse_array(lexer),
        Token::LeftAngle => parse_dictionary(lexer),
        Token::Keyword(keyword) => parse_keyword(keyword),
        Token::RightBracket | Token::RightAngle | Token::Eof => Err(PdfError::InvalidObject),
    }
}

fn parse_keyword(keyword: String) -> Result<Object> {
    match keyword.as_str() {
        "true" => Ok(Object::Boolean(true)),
        "false" => Ok(Object::Boolean(false)),
        "null" => Ok(Object::Null),
        _ => Err(PdfError::InvalidObject),
    }
}

fn parse_array(lexer: &mut Lexer) -> Result<Object> {
    let mut items = Vec::new();
    loop {
        let token = lexer.next_token()?;
        if token == Token::RightBracket {
            break;
        }
        let value = parse_object_from_token(lexer, token)?;
        items.push(value);
    }
    Ok(Object::Array(items))
}

fn parse_dictionary(lexer: &mut Lexer) -> Result<Object> {
    let mut dict = HashMap::new();
    loop {
        let token = lexer.next_token()?;
        if token == Token::RightAngle {
            break;
        }

        let key = match token {
            Token::Name(name) => name,
            _ => return Err(PdfError::InvalidObject),
        };

        let value = parse_object_value(lexer)?;
        dict.insert(key, value);
    }

    Ok(Object::Dictionary(dict))
}

fn parse_number_or_reference(num: f64, lexer: &mut Lexer) -> Result<Object> {
    if let Some(object) = try_parse_reference(num, lexer)? {
        Ok(object)
    } else {
        Ok(Object::Number(num))
    }
}

fn try_parse_reference(num: f64, lexer: &mut Lexer) -> Result<Option<Object>> {
    if num.fract() != 0.0 {
        return Ok(None);
    }

    let checkpoint = lexer.save();
    let second = match lexer.next_token()? {
        Token::Number(value) => value,
        _ => {
            lexer.restore(checkpoint);
            return Ok(None);
        }
    };

    if second.fract() != 0.0 {
        lexer.restore(checkpoint);
        return Ok(None);
    }

    match lexer.next_token()? {
        Token::Keyword(keyword) if keyword == "R" => {
            Ok(Some(Object::Reference(num as u32, second as u16)))
        }
        _ => {
            lexer.restore(checkpoint);
            Ok(None)
        }
    }
}

fn maybe_parse_stream(object: Object, lexer: &mut Lexer) -> Result<Object> {
    match object {
        Object::Dictionary(dict) => {
            let checkpoint = lexer.save();
            match lexer.next_token()? {
                Token::Keyword(keyword) if keyword == "stream" => {
                    let data = if let Some(length) = extract_length(&dict) {
                        lexer.read_stream_data(length)?
                    } else {
                        lexer.read_stream_to_endstream()?
                    };
                    match lexer.next_token()? {
                        Token::Keyword(end_keyword) if end_keyword == "endstream" => {
                            Ok(Object::Stream { dict, data })
                        }
                        _ => Err(PdfError::InvalidObject),
                    }
                }
                _ => {
                    lexer.restore(checkpoint);
                    Ok(Object::Dictionary(dict))
                }
            }
        }
        other => Ok(other),
    }
}

fn extract_length(dict: &HashMap<String, Object>) -> Option<usize> {
    dict.get("Length").and_then(|value| match value {
        Object::Number(n) if *n >= 0.0 => Some(*n as usize),
        _ => None,
    })
}

fn to_u32(value: f64) -> Option<u32> {
    if value.fract() == 0.0 && value >= 0.0 {
        Some(value as u32)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dictionary_object() {
        let data = b"1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj";
        let (_, _, object) = parse_indirect_object(data).expect("object parsed");

        match object {
            Object::Dictionary(dict) => {
                assert!(matches!(dict.get("Type"), Some(Object::Name(name)) if name == "Catalog"));
                assert!(matches!(dict.get("Pages"), Some(Object::Reference(2, 0))));
            }
            _ => panic!("Expected dictionary object"),
        }
    }

    #[test]
    fn parses_stream_object() {
        let data = b"2 0 obj << /Length 5 >> stream\nHello\nendstream\nendobj";
        let (_, _, object) = parse_indirect_object(data).expect("stream parsed");

        match object {
            Object::Stream { dict, data } => {
                assert_eq!(dict.get("Length").unwrap().as_number().unwrap(), 5.0);
                assert_eq!(data, b"Hello".to_vec());
            }
            _ => panic!("Expected stream object"),
        }
    }
}
