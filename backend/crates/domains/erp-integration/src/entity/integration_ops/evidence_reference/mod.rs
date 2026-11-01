//! W29 受控证据引用的精确 grammar、主体关联与集合规范化（INT-E20）。
//!
//! 客户端记录 ID 只接受 `type:id`；已持久化事实引用兼容历史 `type://id`、
//! `type:id`、`type:id:status` 与 `type:id:vN:status`。关联只比较身份 ID，
//! 禁止把类型名、版本段或状态段当作命中。集合编码排序去重，总长不超过 512。
//! 原动作重放把 inbox canonical 与不透明 `business_fact_key` 分开持有，键内 `|`
//! 不得走 `parse_id` 或 [`EvidenceReferenceSet`]。

mod bindings;
mod canonical;
mod parse;
mod record_ref;
mod replay;
mod sets;

pub use bindings::EvidenceSubjectBindings;
pub use canonical::CanonicalEvidenceReference;
pub use record_ref::EvidenceRecordRef;
pub use replay::ReplayOriginalReference;
pub use sets::{CompactEvidenceSet, EvidenceReferenceSet};
