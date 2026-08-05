//! 域 D31 `mall_backfill`：mall_consumption_backfill_job、mall_consumption_backfill_item
//! （页面：W30）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype、`entities::money` 定点类型
//! 与 `common` 基元。字段字典见数据模型 §6.17（回填作业）；公共字段归属按 §4.3 判定：
//! - `mall_consumption_backfill_job` 是回填作业，含作业状态字段
//!   （待执行、运行中、部分完成、完成、失败），定义状态迁移，
//!   实现 [`crate::common::state::DocumentState`]；
//! - `mall_consumption_backfill_item` 是回填明细（新增、重复、待归集、失败），
//!   结果类型是固定枚举，不可变，只提供 `new()`。
//!
//! 回填使用与实时相同的 inbox、业务事实键和正式实体；`T` 前支付只补台账，
//! 不触发供应商下单（§6.17）。`(job_id, business_fact_key)` 唯一由 P2 唯一索引落实；
//! 正式回填批次必须覆盖 `[range_start, T)`、不得制造重叠批次，以及成本口径计数
//! 与报告内容的汇总一致性依赖聚合查询与 P3 调度校验（P3 条目：§6.17 回填覆盖
//! 与成本口径统计）。

pub mod backfill_job;

pub use crate::ids::{MallConsumptionBackfillItemId, MallConsumptionBackfillJobId};
pub use backfill_job::*;
