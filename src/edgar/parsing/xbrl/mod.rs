pub mod parser;

// Re-export types from xml parser
pub use self::parser::xml::{XBRLFiling, FactItem, FactTableRow, DimensionTableRow};
pub use super::types::{FilingDocument, FilingFact, Period};

// Re-export for public API
pub use self::parser::xml::XBRLFiling;
pub use self::parser::xml::FactItem;
pub use self::parser::xml::FactTableRow;
pub use self::parser::xml::DimensionTableRow;
