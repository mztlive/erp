//! 供 Handler 与 Process 复用的导入 HTTP 与应用 DTO。

pub mod legacy_import;
pub mod receipt;

pub use legacy_import::*;
pub use receipt::{optional_text, parse_command_version, parse_receipt_number, required_text};
