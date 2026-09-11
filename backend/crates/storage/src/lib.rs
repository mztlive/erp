//! 统一对象键规则下的 S3 存储。

mod error;
mod path;
mod s3;

pub use error::{Error, Result};
pub use s3::{S3Storage, S3StorageConfig, UploadedPart};
