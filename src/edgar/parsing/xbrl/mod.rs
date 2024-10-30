pub mod parser;

// Re-export types from xml parser
pub use self::parser::xml::{DimensionTableRow, FactItem, FactTableRow, XBRLFiling};

// Re-export core functionality
pub use super::types::{FilingDocument, FilingFact, Period};
