// Lexer (tokenizer) for PDF content.
// Breaks raw PDF bytes into tokens (numbers, names, keywords, etc.).
// PDFs are text-based formats, so we need to tokenize them first before parsing.

use crate::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Name(String),
    String(Vec<u8>),
    Keyword(String),
    LeftBracket,
    RightBracket,
    LeftAngle,
    RightAngle,
    Eof,
}

pub struct Lexer {
    input: Vec<u8>,
    position: usize,
}

impl Lexer {
    pub fn new(input: Vec<u8>) -> Self {
        Lexer { input, position: 0 }
    }

    pub fn next_token(&mut self) -> Result<Token> {
        // TODO: Implement tokenization
        Ok(Token::Eof)
    }
}
