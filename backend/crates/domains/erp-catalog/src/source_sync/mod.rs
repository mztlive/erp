//! 商品展示资料的字段责任、来源基线与差异确认合同。
pub(crate) mod command;
mod model;
pub mod repository;
mod rules;

pub use model::*;

#[cfg(test)]
mod tests;
