//! W29 本域动作、终态、版本与不可变差异决定写入。
pub mod action;
pub mod complete;
pub mod direct;
pub mod guard;

use erp_core::common::time::Instant;

use crate::dto::{ControlledEvidenceRef, DirectReconciliationStatus, IntegrationActionOutcome};
use crate::entity::integration_ops::{
    ReconciliationDifference, ReconciliationDifferenceId, ReconciliationDifferenceResolution,
    ReconciliationDifferenceResolutionId, ResolutionAction,
};
use crate::{Error, Result};

/// 本域执行后供流程回执使用的决定事实。
#[derive(Debug, Clone)]
pub struct DirectFact {
    /// 不可变决定记录使用的领域动作。
    pub action: ResolutionAction,
    /// 决定记录中的已验证证据引用。
    pub evidence_reference: Option<String>,
    /// 供直接决定回执使用的当前状态。
    pub resulting_status: DirectReconciliationStatus,
    /// 本次动作的稳定结果代码。
    pub outcome: IntegrationActionOutcome,
    /// 权威业务结果的稳定引用。
    pub business_result_reference: Option<String>,
    /// 已逐条验证的受控证据，保持请求顺序。
    pub verified_evidence: Vec<ControlledEvidenceRef>,
}

/// 经领域追加工厂形成决定记录（序号递增与状态派生归领域，时间与身份由调用方注入）。
///
/// # 参数
/// * `difference` - 所属对账差异
/// * `latest` - 当前最新决定；`None` 表示首条追加
/// * `fact` - 已验证的决定事实（动作与证据引用）
/// * `record_id` - 记录主键（调用方收据 ID）
/// * `actor_id` - 决定人
///
/// # 返回
/// 返回新建的不可变追加式决定记录。
///
/// # 错误
/// 序号已达上限时返回 `ConflictError`，其余领域约束失败保持领域逻辑错误。
///
/// # 约束
/// 序号上限文案由领域拥有，此处只做错误类别映射，不维护第二份上限规则。
fn append_resolution(
    difference: &ReconciliationDifference,
    latest: Option<&ReconciliationDifferenceResolution>,
    fact: &DirectFact,
    record_id: &str,
    actor_id: &str,
) -> Result<ReconciliationDifferenceResolution> {
    ReconciliationDifferenceResolution::append(
        ReconciliationDifferenceResolutionId::new(record_id.to_string()),
        ReconciliationDifferenceId::new(difference.base.id.clone()),
        latest,
        fact.action,
        fact.evidence_reference.clone(),
        actor_id.to_string(),
        Instant::now(),
    )
    .map_err(|error| {
        let message = error.to_string();
        if message == "差异决定序号已达上限" {
            Error::ConflictError(message)
        } else {
            Error::Logic(error)
        }
    })
}
