//! 库存调整普通撤回授权：原提交人和运行管理员沿调用方快照重新证明身份。

use std::sync::Arc;

use application_core::AuditActor;
use bpm::model::ApprovalProcessInstance;
use erp_identity::AuthorizationService;
use erp_workflow::ApprovalIntegrationExt;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use super::InventoryAdjustmentProcess;
use crate::{Error, Result};

/// 普通撤回的授权身份，用于审计应急代办路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelAuthority {
    Submitter,
    RuntimeAdmin,
}

/// 事务内重新证明的普通撤回授权事实。
pub struct CancelAuthorization {
    pub(crate) authority: CancelAuthority,
    pub(crate) submitted_by: String,
    pub(crate) responsible_org_id: String,
}

impl CancelAuthority {
    /// 返回撤回授权身份的展示名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回原提交人或审批管理员的中文名称。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Submitter => "原提交人",
            Self::RuntimeAdmin => "审批管理员",
        }
    }
}

/// 读取取消计划所需的冻结提交人及责任组织；写事务另行执行完整授权。
/// # 参数
/// service、instance为当前用例与精确实例，actor为待执行操作人，executor为已授权准备快照。
/// # 返回
/// 仅用于准备计划的责任事实，不代表授权通过。
/// # 错误
/// 冻结快照缺失或与实例不匹配时返回错误。
pub(crate) async fn cancel_authority_facts(
    service: &InventoryAdjustmentProcess,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CancelAuthorization> {
    let snapshot = cancel_authorization_snapshot(&service.db, instance, executor).await?;
    Ok(CancelAuthorization {
        authority: if snapshot.payload.submitted_by == actor.id() {
            CancelAuthority::Submitter
        } else {
            CancelAuthority::RuntimeAdmin
        },
        submitted_by: snapshot.payload.submitted_by,
        responsible_org_id: snapshot.payload.responsible_org_id,
    })
}

/// 原撤回授权顺序使用应用已装配的工作流 Port，禁止重建授权源。
pub(crate) async fn authorize_with(
    identity_service: &Arc<AuthorizationService>,
    db: &Database,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CancelAuthorization> {
    let snapshot = cancel_authorization_snapshot(db, instance, executor).await?;
    super::cancel_access::require_cancel(db, identity_service, actor, &snapshot, executor).await?;
    let authority = if snapshot.payload.submitted_by == actor.id() {
        CancelAuthority::Submitter
    } else {
        CancelAuthority::RuntimeAdmin
    };
    Ok(CancelAuthorization {
        authority,
        submitted_by: snapshot.payload.submitted_by,
        responsible_org_id: snapshot.payload.responsible_org_id,
    })
}

/// 在原授权步骤的快照读取位置，验证冻结业务对象仍对应同一运行实例。
async fn cancel_authorization_snapshot(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少冻结业务快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
    Ok(snapshot)
}
