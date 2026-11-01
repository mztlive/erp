//! ERP HTTP API 库：应用状态、Handler 与路由。
//!
//! 二进制入口只负责进程生命周期；本库承载已接线的 HTTP 面，避免二进制目标
//! 把公开构造器与 Handler 辅助类型误判为 dead_code。

pub mod app_state;
pub mod core;

mod indexes;
pub mod runtime;

pub mod permissions;

#[cfg(test)]
#[path = "../build_catalog.rs"]
mod build_catalog;
#[cfg(test)]
#[path = "../build_permissions.rs"]
mod build_permissions;

pub mod approval_dispatch;

pub mod wiring;
