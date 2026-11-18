//! 库存调整普通撤回授权：原提交人和运行管理员沿调用方快照重新证明身份。

use application_core::AuditActor;
use bpm::model::ApprovalProcessInstance;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::inventory;
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
    // 将撤回时使用的身份转换成中文名称，区分原提交人和审批管理员。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Submitter => "原提交人",
            Self::RuntimeAdmin => "审批管理员",
        }
    }
}

// 读取取消计划所需的冻结提交人及责任组织；写事务另行执行完整授权。
//
// service、instance为当前用例与精确实例，actor为待执行操作人，executor为已授权准备快照。仅用于准备计划的责任事实，不代表授权通过。
//
// 冻结快照缺失或与实例不匹配时返回错误。
pub(crate) async fn cancel_authority_facts(
    service: &InventoryAdjustmentProcess,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CancelAuthorization> {
    let snapshot = cancel_authorization_snapshot(&service.db, instance, executor).await?;
    Ok(cancel_facts(snapshot, actor))
}

// 写事务按已校验的审批实例重新证明根资格和完整来源资格。
pub(crate) async fn authorize_with(
    identity_service: &AuthorizationService,
    db: &Database,
    instance: &ApprovalProcessInstance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CancelAuthorization> {
    let snapshot = cancel_authorization_snapshot(db, instance, executor).await?;
    identity_service
        .require(
            actor,
            &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
            AccessTarget::record(snapshot.approval_process_instance_id.as_ref())?,
            executor,
        )
        .await?;
    Ok(cancel_facts(snapshot, actor))
}

fn cancel_facts(snapshot: ApprovalSubjectSnapshot, actor: &AuditActor) -> CancelAuthorization {
    CancelAuthorization {
        authority: if snapshot.payload.submitted_by == actor.id() {
            CancelAuthority::Submitter
        } else {
            CancelAuthority::RuntimeAdmin
        },
        submitted_by: snapshot.payload.submitted_by,
        responsible_org_id: snapshot.payload.responsible_org_id,
    }
}

// 在原授权步骤的快照读取位置，验证冻结业务对象仍对应同一运行实例。
async fn cancel_authorization_snapshot(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| {
            Error::ConflictError("找不到本次审批提交时的单据资料，请联系管理员核对".to_string())
        })?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| Error::ConflictError("审批记录与提交时的单据资料不符，请联系管理员核对".to_string()))?;
    Ok(snapshot)
}
