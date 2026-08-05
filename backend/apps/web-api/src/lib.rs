//! web-api 库目标（P0-5 垂直样板新增）。
//!
//! 原包为纯二进制（仅 `main.rs`），集成测试（`tests/*.rs`）无法链接二进制
//! crate；本文件以同一源码树声明库目标，供 HTTP 集成测试组装
//! `routes::create(AppState)` 使用。`main.rs` 保持原样，二者互不影响。
//!
//! 建议随「地基修订 PR」正式合入（若未来域测试普遍依赖，可长期保留）。
//!
//! # 目标级 lint 说明
//!
//! 同一源码树在二进制目标下只由 `main.rs` 消费，在库目标下没有对应消费方，
//! 因此产生两条仅在库目标出现的告警（`-D warnings` 门禁下必须显式关闭）：
//! - `unused_imports`：`core/tracing/mod.rs` 的 `pub(crate) use` 只被 `main.rs` 使用；
//! - `private_interfaces`：`core/errors.rs` 的 `Error::RateLimited` 字段类型
//!   `rate_limit::Error` 为 `pub(crate)`，二进制目标无外部接口不触发，库目标触发。
//!
//! 两项均为二进制/库双目标共存的固有现象，与代码质量无关。

#![allow(unused_imports, private_interfaces, dead_code)]

pub mod app_state;
pub mod core;
