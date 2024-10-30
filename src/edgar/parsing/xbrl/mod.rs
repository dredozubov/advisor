pub mod parser;
pub use parser::xml::{XBRLFiling, FactItem, FactTableRow, DimensionTableRow};

// Re-export core functionality
pub use super::types::{FilingDocument, FilingFact, Period};
