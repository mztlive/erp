//! 退货逆向事实、数量规则、凭证合同与本域持久化。

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use error::{Error, Result};
