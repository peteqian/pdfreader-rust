pub mod content;
pub mod content_parser;
pub mod error;
pub mod filters;
pub mod fonts;
pub mod lexer;
pub mod objects;
pub mod stream;
pub mod xref;

pub use error::{PdfError, Result};

// Re-export commonly used types
pub use content::PageText;
