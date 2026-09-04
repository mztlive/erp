//! 域 D31 `mall_backfill`：mall_consumption_backfill_job、mall_consumption_backfill_item
//! （页面：W30）。
//!
//! 实体层只复用 `entities::ids`、`entities::money`、`common` 基元，以及 D29
//! 商城关键事实的固定类型/处理状态枚举，用于确定性派生回填结果与成本口径。
//! 字段字典见数据模型 §6.17（回填作业）；公共字段归属按 §4.3 判定：
//! - `mall_consumption_backfill_job` 是回填作业，含作业状态字段
//!   （待执行、运行中、部分完成、完成、失败），定义状态迁移，
//!   实现 [`crate::common::state::DocumentState`]；
//! - `mall_consumption_backfill_item` 是回填明细（新增、重复、待归集、失败），
//!   结果类型是固定枚举，不可变，只提供 `new()`。
//!
//! 回填使用与实时相同的 inbox、业务事实键和正式实体；`T` 前支付只补台账，
//! 不触发供应商下单（§6.17）。`[range_start, T)`、重叠阻断状态与事实分类由本域
//! 类型确定；`(job_id, business_fact_key)` 唯一索引、重叠批次查询及成本汇总写入
//! 仍由 P2 Repository 与 P3 Service 编排。

pub mod backfill_job;
pub mod backfill_progress;

pub use crate::ids::{MallConsumptionBackfillItemId, MallConsumptionBackfillJobId};
pub use backfill_job::*;
pub use backfill_progress::BackfillProgress;
