//! 采购订单、采购责任与本域持久化合同。

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use error::{Error, Result};
