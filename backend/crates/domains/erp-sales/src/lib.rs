//! 销售单、冻结提交、正式版本与销售变更。

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use error::{Error, Result};
