//! Import HTTP/application DTOs reused by handlers and processes.

pub mod legacy_import;
pub mod receipt;

pub use legacy_import::*;
pub use receipt::{optional_text, parse_command_version, parse_receipt_number, required_text};
