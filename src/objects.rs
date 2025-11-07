// PDF object types and representations.
// Represents the different data structures found in PDFs (numbers, strings, dictionaries, arrays, etc.).
// These are what we parse from tokens.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Object {
    Null,
    Boolean(bool),
    Number(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Object>),
    Dictionary(HashMap<String, Object>),
    Stream { dict: HashMap<String, Object>, data: Vec<u8> },
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
