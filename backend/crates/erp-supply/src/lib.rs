//! 供应供给、连接能力、供应商履约和结算的领域规则与持久化。

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use error::{Error, Result, known_duplicate_index_message};
