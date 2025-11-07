use std::fmt;

#[derive(Debug)]
pub enum PdfError {
    IoError(std::io::Error),
    InvalidHeader,
    InvalidXref,
    InvalidObject,
    MissingTrailer,
    InvalidToken,
    Other(String),
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PdfError::IoError(e) => write!(f, "IO error: {}", e),
            PdfError::InvalidHeader => write!(f, "Invalid PDF header"),
            PdfError::InvalidXref => write!(f, "Invalid cross-reference table"),
            PdfError::InvalidObject => write!(f, "Invalid PDF object"),
            PdfError::MissingTrailer => write!(f, "Missing trailer dictionary"),
            PdfError::InvalidToken => write!(f, "Invalid PDF token"),
            PdfError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<std::io::Error> for PdfError {
    fn from(err: std::io::Error) -> Self {
        PdfError::IoError(err)
    }
}

pub type Result<T> = std::result::Result<T, PdfError>;
