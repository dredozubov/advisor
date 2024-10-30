pub mod types;
pub mod xbrl;

pub use types::{FilingDocument, FilingFact};
pub use xbrl::extract_facts;
