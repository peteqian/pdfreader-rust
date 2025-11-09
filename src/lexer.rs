// Lexer (tokenizer) for PDF content.
// Breaks raw PDF bytes into tokens (numbers, names, keywords, etc.).
// PDFs are text-based formats, so we need to tokenize them first before parsing.

use crate::{PdfError, Result};

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
        self.skip_whitespace_and_comments();

        if self.position >= self.input.len() {
            return Ok(Token::Eof);
        }

        let byte = self.input[self.position];

        match byte {
            b'/' => self.read_name(),
            b'(' => self.read_literal_string(),
            b'<' => {
                if self.peek_next() == Some(b'<') {
                    self.position += 2;
                    Ok(Token::LeftAngle)
                } else {
                    self.position += 1;
                    self.read_hex_string()
                }
            }
            b'>' => {
                if self.peek_next() == Some(b'>') {
                    self.position += 2;
                    Ok(Token::RightAngle)
                } else {
                    Err(PdfError::InvalidToken)
                }
            }
            b'[' => {
                self.position += 1;
                Ok(Token::LeftBracket)
            }
            b']' => {
                self.position += 1;
                Ok(Token::RightBracket)
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => self.read_number(),
            _ => {
                if is_delimiter(byte) {
                    Err(PdfError::InvalidToken)
                } else {
                    self.read_keyword()
                }
            }
        }
    }

    pub fn save(&self) -> usize {
        self.position
    }

    pub fn restore(&mut self, pos: usize) {
        self.position = pos.min(self.input.len());
    }

    pub fn read_stream_data(&mut self, length: usize) -> Result<Vec<u8>> {
        self.skip_newline();
        if self.position + length > self.input.len() {
            return Err(PdfError::InvalidObject);
        }

        let data = self.input[self.position..self.position + length].to_vec();
        self.position += length;
        self.skip_newline();
        Ok(data)
    }

    pub fn read_stream_to_endstream(&mut self) -> Result<Vec<u8>> {
        self.skip_newline();
        let marker = b"endstream";
        let remaining = &self.input[self.position..];
        let idx = find_subsequence(remaining, marker).ok_or(PdfError::InvalidObject)?;
        let mut data = remaining[..idx].to_vec();
        while data
            .last()
            .map(|b| *b == b'\n' || *b == b'\r')
            .unwrap_or(false)
        {
            data.pop();
        }
        self.position += idx;
        Ok(data)
    }

    fn peek_next(&self) -> Option<u8> {
        if self.position + 1 < self.input.len() {
            Some(self.input[self.position + 1])
        } else {
            None
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.position < self.input.len() {
            let byte = self.input[self.position];
            if is_whitespace(byte) {
                self.position += 1;
                continue;
            }
            if byte == b'%' {
                self.position += 1;
                while self.position < self.input.len() {
                    let b = self.input[self.position];
                    if b == b'\n' || b == b'\r' {
                        break;
                    }
                    self.position += 1;
                }
                continue;
            }
            break;
        }
    }

    fn skip_newline(&mut self) {
        if self.position < self.input.len() {
            if self.input[self.position] == b'\r' {
                self.position += 1;
                if self.position < self.input.len() && self.input[self.position] == b'\n' {
                    self.position += 1;
                }
            } else if self.input[self.position] == b'\n' {
                self.position += 1;
            }
        }
    }

    fn read_number(&mut self) -> Result<Token> {
        let start = self.position;
        if self.input[self.position] == b'+' || self.input[self.position] == b'-' {
            self.position += 1;
        }

        while self.position < self.input.len() {
            let b = self.input[self.position];
            if (b'0'..=b'9').contains(&b) {
                self.position += 1;
            } else {
                break;
            }
        }

        if self.position < self.input.len() && self.input[self.position] == b'.' {
            self.position += 1;
            while self.position < self.input.len() {
                let b = self.input[self.position];
                if (b'0'..=b'9').contains(&b) {
                    self.position += 1;
                } else {
                    break;
                }
            }
        }

        let slice = &self.input[start..self.position];
        let number = std::str::from_utf8(slice)
            .map_err(|_| PdfError::InvalidToken)?
            .parse::<f64>()
            .map_err(|_| PdfError::InvalidToken)?;

        Ok(Token::Number(number))
    }

    fn read_name(&mut self) -> Result<Token> {
        self.position += 1; // Skip '/'
        let start = self.position;
        let mut name = Vec::new();

        while self.position < self.input.len() {
            let b = self.input[self.position];
            if is_delimiter(b) || is_whitespace(b) {
                break;
            }

            if b == b'#' {
                if self.position + 2 >= self.input.len() {
                    return Err(PdfError::InvalidToken);
                }
                let hex = &self.input[self.position + 1..self.position + 3];
                let value = hex_to_byte(hex)?;
                name.push(value);
                self.position += 3;
                continue;
            }

            name.push(b);
            self.position += 1;
        }

        if name.is_empty() {
            name.extend_from_slice(&self.input[start..self.position]);
        }

        let value = String::from_utf8(name).map_err(|_| PdfError::InvalidToken)?;
        Ok(Token::Name(value))
    }

    fn read_literal_string(&mut self) -> Result<Token> {
        self.position += 1; // Skip '('
        let mut depth = 1;
        let mut result = Vec::new();

        while self.position < self.input.len() {
            let b = self.input[self.position];
            self.position += 1;

            match b {
                b'(' => {
                    depth += 1;
                    result.push(b'(');
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    result.push(b')');
                }
                b'\\' => {
                    let escaped = self.read_escape_sequence()?;
                    result.extend_from_slice(&escaped);
                }
                _ => result.push(b),
            }
        }

        if depth != 0 {
            return Err(PdfError::InvalidToken);
        }

        Ok(Token::String(result))
    }

    fn read_escape_sequence(&mut self) -> Result<Vec<u8>> {
        if self.position >= self.input.len() {
            return Err(PdfError::InvalidToken);
        }

        let b = self.input[self.position];
        self.position += 1;

        let value = match b {
            b'n' => vec![b'\n'],
            b'r' => vec![b'\r'],
            b't' => vec![b'\t'],
            b'b' => vec![b'\x08'],
            b'f' => vec![b'\x0C'],
            b'(' => vec![b'('],
            b')' => vec![b')'],
            b'\\' => vec![b'\\'],
            b'\n' => Vec::new(),
            b'\r' => {
                if self.position < self.input.len() && self.input[self.position] == b'\n' {
                    self.position += 1;
                }
                Vec::new()
            }
            b'0'..=b'7' => {
                let mut oct_digits = vec![b];
                for _ in 0..2 {
                    if self.position < self.input.len() {
                        let next = self.input[self.position];
                        if (b'0'..=b'7').contains(&next) {
                            oct_digits.push(next);
                            self.position += 1;
                            continue;
                        }
                    }
                    break;
                }
                let octal = std::str::from_utf8(&oct_digits)
                    .map_err(|_| PdfError::InvalidToken)?
                    .parse::<u8>()
                    .map_err(|_| PdfError::InvalidToken)?;
                vec![octal]
            }
            _ => vec![b],
        };

        Ok(value)
    }

    fn read_hex_string(&mut self) -> Result<Token> {
        let mut data = Vec::new();
        let mut buffer = Vec::new();

        while self.position < self.input.len() {
            let b = self.input[self.position];
            if b == b'>' {
                break;
            }
            if is_whitespace(b) {
                self.position += 1;
                continue;
            }
            buffer.push(b);
            self.position += 1;
        }

        if self.position >= self.input.len() || self.input[self.position] != b'>' {
            return Err(PdfError::InvalidToken);
        }
        self.position += 1;

        if buffer.len() % 2 != 0 {
            buffer.push(b'0');
        }

        for chunk in buffer.chunks(2) {
            let byte = hex_to_byte(chunk)?;
            data.push(byte);
        }

        Ok(Token::String(data))
    }

    fn read_keyword(&mut self) -> Result<Token> {
        let start = self.position;
        while self.position < self.input.len() {
            let b = self.input[self.position];
            if is_delimiter(b) || is_whitespace(b) {
                break;
            }
            self.position += 1;
        }

        let slice = &self.input[start..self.position];
        let keyword = String::from_utf8(slice.to_vec()).map_err(|_| PdfError::InvalidToken)?;
        Ok(Token::Keyword(keyword))
    }
}

fn hex_to_byte(bytes: &[u8]) -> Result<u8> {
    if bytes.len() < 2 {
        return Err(PdfError::InvalidToken);
    }
    let high = hex_char_to_val(bytes[0])?;
    let low = hex_char_to_val(bytes[1])?;
    Ok((high << 4) | low)
}

fn hex_char_to_val(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(10 + (b - b'a')),
        b'A'..=b'F' => Ok(10 + (b - b'A')),
        _ => Err(PdfError::InvalidToken),
    }
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, b'\x00' | b'\t' | b'\n' | b'\r' | b'\x0C' | b' ')
}

fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_basic_tokens() {
        let input = b"/Name 42 -3.5 (Hello) [true false] << /Type /Page >>".to_vec();
        let mut lexer = Lexer::new(input);

        assert_eq!(lexer.next_token().unwrap(), Token::Name("Name".into()));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(42.0));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(-3.5));
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String(b"Hello".to_vec())
        );
        assert_eq!(lexer.next_token().unwrap(), Token::LeftBracket);
        assert_eq!(lexer.next_token().unwrap(), Token::Keyword("true".into()));
        assert_eq!(lexer.next_token().unwrap(), Token::Keyword("false".into()));
        assert_eq!(lexer.next_token().unwrap(), Token::RightBracket);
        assert_eq!(lexer.next_token().unwrap(), Token::LeftAngle);
        assert_eq!(lexer.next_token().unwrap(), Token::Name("Type".into()));
        assert_eq!(lexer.next_token().unwrap(), Token::Name("Page".into()));
        assert_eq!(lexer.next_token().unwrap(), Token::RightAngle);
    }

    #[test]
    fn parses_hex_and_literal_strings() {
        let input = b"<48656C6C6F> (Hi\\nThere)".to_vec();
        let mut lexer = Lexer::new(input);

        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String(b"Hello".to_vec())
        );
        assert_eq!(
            lexer.next_token().unwrap(),
            Token::String(b"Hi\nThere".to_vec())
        );
    }
}
