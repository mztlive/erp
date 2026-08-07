//! 统一对象键规则下的本地文件与 S3 存储。

mod error;
mod local;
mod path;
mod s3;

pub use error::{Error, Result};
pub use local::LocalStorage;
pub use s3::{S3Storage, S3StorageConfig};
