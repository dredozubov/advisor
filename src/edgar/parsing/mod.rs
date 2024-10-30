pub mod types;
pub mod xbrl;

pub use types::{FilingDocument, FilingFact, FilingSection, SectionType};
pub use xbrl::{parse_filing, extract_facts};
