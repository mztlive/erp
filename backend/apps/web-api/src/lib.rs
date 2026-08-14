//! web-api 库目标。
//!
//! HTTP 路由、handler 与进程启动所需的公开类型都从本库导出。
//! `src/main.rs` 与集成测试只链接本库，不再各自编译一份 `mod core`。
//!
//! # 目标级 lint 说明
//!
//! 库目标会触发 `private_interfaces`：`core/errors.rs` 的 `Error::RateLimited`
//! 字段类型 `rate_limit::Error` 为 `pub(crate)`。该项与对外 HTTP 契约无关，
//! 在 crate 根关闭以免 `-D warnings` 误伤。

#![allow(private_interfaces, dead_code)]

pub mod app_state;
pub mod core;
