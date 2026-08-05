//! 域 D20 `cost`：cost_entry、cost_allocation（页面：W16）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 common 基元。
//! 公共字段归属按 §4.3 判定：
//! - `cost_entry` / `cost_allocation` 是正式事实（§4.5 不设业务软删除），按
//!   §6.10 字段字典建模；§6.10 字典未含 FactBase 全部语义字段（fact_no/
//!   recorded_at/recorded_by/source_type/source_reference/reason_code/
//!   reason_text），因此不组合 FactBase，仅用 `BaseModel` 持久化元数据；
//! - 成本无 §7 状态机（阶段由 `cost_stage` 固定枚举表达），不发明状态机。

pub mod cost_allocation;
pub mod cost_entry;

pub use cost_allocation::*;
pub use cost_entry::*;
