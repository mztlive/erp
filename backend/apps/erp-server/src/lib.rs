//! ERP 服务端应用：共享装配、业务 HTTP 与独立 Worker。
pub mod bootstrap;
pub mod http;
mod indexes;
pub mod runtime;
pub(crate) mod worker;

pub mod permissions;

#[cfg(test)]
#[path = "../build_catalog.rs"]
mod build_catalog;
#[cfg(test)]
#[path = "../build_permissions.rs"]
mod build_permissions;

pub mod wiring;
