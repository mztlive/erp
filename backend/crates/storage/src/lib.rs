//! 受基础目录约束的本地文件存储。

mod error;
mod local;

pub use error::{Error, Result};
pub use local::LocalStorage;
