//! 供应商售后写意图与结算来源收件；不得把来源受理解释为财务完成。
mod refund;
mod repository;
mod settlement;
mod source;
mod view;
pub use refund::*;
pub use repository::*;
pub use settlement::*;
use sha2::{Digest, Sha256};
pub use source::validate_settlement_source;
pub use view::*;

pub(crate) fn identity(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.len().to_string().as_bytes());
        hash.update(b":");
        hash.update(part.as_bytes());
    }
    hex::encode(hash.finalize())
}
