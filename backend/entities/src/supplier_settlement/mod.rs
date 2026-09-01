//! 域 D33 `supplier_settlement`：supplier_settlement_statement、supplier_settlement_item、
//! supplier_settlement_difference（页面：W27）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元（数据模型 §3.1）。
//! 字段字典见 §6.20；结算单是正式单据，确认后形成应付（§8.4 第 6 条），经办/复核岗位
//! 分离在实体层固化；完成、取消和退款事实均参与结算，不按可变当前状态猜测历史金额
//! （§6.20）。
//!
//! 本域在 §8.4 第 6 条中实现实体层可判定的部分：结算单金额与差异恒等、经办/复核分离、
//! 确认状态与应付账户/确认时间的一致性、结算明细构成恒等、差异处理结果的成组约束；
//! 锁定结算单及差异处理结果、追加最终成本差额、形成结算单应付与更新状态的事务编排
//! 留给 P3。

pub mod difference;
pub mod draft_snapshot;
pub mod evidence;
pub mod item;
pub mod source_evidence;
pub mod statement;

pub use crate::ids::{
    SupplierSettlementDifferenceId, SupplierSettlementItemId, SupplierSettlementStatementId,
};
pub use difference::{
    SettlementDifferenceConclusion, SettlementDifferenceConclusionKind, SettlementDifferenceStatus,
    SettlementDifferenceType, SupplierSettlementDifference, SupplierSettlementDifferenceData,
    SupplierSettlementDifferenceUpdate,
};
pub use draft_snapshot::SupplierSettlementDraftSnapshot;
pub use evidence::{SupplierSettlementDifferenceEvidence, SupplierSettlementDifferenceEvidenceData};
pub use item::{SettlementCostDelta, SupplierSettlementItem, SupplierSettlementItemData};
pub use source_evidence::{
    SettlementAmountComponents, SettlementCancelEvidence, SettlementPeriod, SettlementSourceFactType,
    SupplierSettlementSourceEvidence, SupplierSettlementSourceEvidenceData,
    SupplierSettlementSourceEvidenceLine, SupplierSettlementSourceEvidenceLineData, SETTLEMENT_TIMEZONE,
};
pub use statement::{
    SettlementReviewDecision, SettlementReviewResult, SettlementStatus, SupplierSettlementSnapshotUpdate,
    SupplierSettlementStatement, SupplierSettlementStatementData, SupplierSettlementStatementUpdate,
};
