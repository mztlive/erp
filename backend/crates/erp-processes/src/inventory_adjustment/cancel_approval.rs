//! 库存调整普通撤回入口：调用统一取消编排并组织事务边界。
//!
//! 运行事实加载与取消输入构图见 [`cancel_runtime`]；事务持久化与受阻取消见
//! [`cancel_persist`]。本入口只做 `with_transaction` 包裹、回放恢复与领域动作
//! 执行，业务步骤均为收 executor 的函数，可被其他用例复用。

use application_core::AuditActor;
use bpm::model::IdempotencyKey;
use erp_core::common::time::Instant;
use erp_inventory::{CancelStockAdjustmentApprovalRequest, StockAdjustment, StockAdjustmentView};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_document_cancel};
use persistence_core::NoTransaction;
use validator::Validate;

use super::InventoryAdjustmentService;
use super::adapter::{
    execute_stock_adjustment_domain_action, require_frozen_binding, stock_adjustment_adapter,
};
use super::approval_query::load_approval_binding;
#[allow(unused_imports)]
pub(crate) use super::cancel_persist::{
    StockAdjustmentCancelPersistInput, cancel_stock_adjustment_approval_apply,
    persist_stock_adjustment_cancel,
};
pub(crate) use super::cancel_runtime::{
    LoadedCancelRuntime, actor_can_cancel, build_stock_adjustment_cancel_input, committed_cancel_replay,
    ensure_cancel_authorized, ensure_cancel_instance_binding, ensure_cancel_instance_subject,
    ensure_cancel_runtime_versions, ensure_expected_version, ensure_stock_adjustment_open_task_identity,
    load_cancel_instance, load_cancel_runtime, normalize_cancel_reason,
    normalize_document_cancel_notification, recover_cancel_replay,
};
use crate::{Error, Result};

impl InventoryAdjustmentService {
    /// 撤回审批中的库存调整单，回到草稿且保持审批主题版本不变。
    ///
    /// 本方法是合同 §4.4.4 签署的普通撤回端口。它以审批实例作为稳定幂等
    /// 作用域，先回读收据，再校验完整运行时 CAS 和业务撤回规则。新命令在单一
    /// MongoDB 事务内写回业务单据、实例、执行、收据、全部开放任务和审计。
    ///
    /// # 参数
    /// * `id` - 库存调整单主键
    /// * `req` - 单据/运行事实期望版本、原因和幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回调用人当前可读的最新库存调整单视图；同载荷回放不重复写入。
    ///
    /// # 错误
    /// 非原提交人或无范围运行管理员、动作/对象范围不足、版本变化、非审批中、
    /// 原因或幂等键非法，或任一事务写入失败时返回错误。
    #[tracing::instrument(
        name = "inventory.stock_adjustment_cancel_approval",
        skip_all,
        fields(layer = "service", domain = "inventory", operation = "stock_adjustment_cancel_approval")
    )]
    pub async fn cancel_stock_adjustment_approval(
        &self,
        id: &str,
        req: CancelStockAdjustmentApprovalRequest,
        actor: &AuditActor,
    ) -> Result<StockAdjustmentView> {
        req.validate()?;
        let reason = normalize_cancel_reason(&req.reason)?;
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        if let Some(view) = committed_cancel_replay(self, id, &req, &reason, &idempotency_key, actor).await? {
            return Ok(view);
        }
        let target = load_cancel_target(self, id, &req, actor).await?;
        let input =
            build_cancel_persist_input(self, target, &req, actor, reason.clone(), idempotency_key.clone())?;
        let result = persist_stock_adjustment_cancel(&self.db, input).await;
        match result {
            Ok(adjustment) => Ok(adjustment.into()),
            Err(error) => {
                if let Some(view) =
                    recover_cancel_replay(self, id, &req, &reason, &idempotency_key, actor).await?
                {
                    return Ok(view);
                }
                Err(error)
            },
        }
    }
}

/// 事务外一次性加载并校验撤回目标（实例/单据/绑定/运行事实）。
async fn load_cancel_target(
    service: &InventoryAdjustmentService,
    id: &str,
    req: &CancelStockAdjustmentApprovalRequest,
    actor: &AuditActor,
) -> Result<LoadedCancelTarget> {
    let instance = load_cancel_instance(&service.db, &req.approval_process_instance_id).await?;
    ensure_cancel_instance_subject(&instance, id, req.expected_subject_version)?;
    let authorization = ensure_cancel_authorized(service, &instance, actor).await?;
    let adjustment = service.inventory().load_stock_adjustment(id).await?;
    ensure_expected_version("库存调整单", req.expected_version, adjustment.base.version)?;
    if adjustment.approval_subject_version != req.expected_subject_version {
        return Err(Error::ConflictError("库存调整审批主题版本已变化，请刷新后重试".to_string()));
    }
    let binding = load_approval_binding(&service.db, id, &mut NoTransaction).await?;
    let binding = require_frozen_binding(binding.as_ref())?.clone();
    ensure_cancel_instance_binding(&instance, &binding)?;
    let runtime = load_cancel_runtime(&service.db, &binding, instance).await?;
    ensure_cancel_runtime_versions(&runtime, req, &authorization)?;
    Ok(LoadedCancelTarget { adjustment, binding, runtime })
}

/// 事务外构图取消计划、执行领域动作并组装持久化输入。
fn build_cancel_persist_input(
    service: &InventoryAdjustmentService,
    target: LoadedCancelTarget,
    req: &CancelStockAdjustmentApprovalRequest,
    actor: &AuditActor,
    reason: String,
    idempotency_key: IdempotencyKey,
) -> Result<StockAdjustmentCancelPersistInput> {
    let LoadedCancelTarget { mut adjustment, binding, runtime } = target;
    let now = Instant::now();
    let input =
        build_stock_adjustment_cancel_input(&runtime, req, actor.id(), &reason, &idempotency_key, None, now)?;
    let PreparedExecution::Apply(mut writes) = prepare_document_cancel(input, req.expected_version)? else {
        return Err(Error::Internal("新库存调整撤回不得进入回放分支".to_string()));
    };
    normalize_document_cancel_notification(&mut writes)?;
    let adapter = stock_adjustment_adapter()?;
    execute_stock_adjustment_domain_action(&mut adjustment, adapter.cancel_action)?;
    Ok(StockAdjustmentCancelPersistInput {
        rbac: service.rbac.clone(),
        document_no: adjustment.adjustment_no.clone(),
        current_approver_id: runtime.current.assignee_participant_id.as_str().to_string(),
        current_approver_name: runtime.current.assignee_name_snapshot.clone(),
        authorization_instance: runtime.instance,
        authorization_execution: runtime.current,
        open_tasks: runtime.open_tasks,
        actor: actor.clone(),
        adjustment,
        writes,
        binding,
        reason,
        now,
    })
}

/// 入口装配所需的已校验撤回目标。
struct LoadedCancelTarget {
    /// 版本与状态均已校验的库存调整单。
    adjustment: StockAdjustment,
    /// 已冻结的审批定义绑定。
    binding: ApprovalDefinitionBinding,
    /// 已校验版本的取消运行事实。
    runtime: LoadedCancelRuntime,
}

#[cfg(test)]
pub(crate) use super::cancel_runtime::{
    cancel_audit_matches_instance, cancel_audit_message_prefix, cancel_replay_actor_mismatch,
};

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::{NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp};

    use super::{cancel_audit_matches_instance, cancel_audit_message_prefix, cancel_replay_actor_mismatch};
    use crate::Error;

    /// 库存普通撤回必须复用统一 V3/legacy 身份，禁止退回 raw key 或独立摘要。
    #[test]
    fn cancel_replay_uses_typed_identity_and_exact_scope_candidates() {
        let production: String = [
            include_str!("cancel_approval.rs"),
            include_str!("cancel_runtime.rs"),
            include_str!("cancel_persist.rs"),
        ]
        .iter()
        .map(|source| source.split("#[cfg(test)]").next().expect("生产代码必须存在"))
        .collect();
        assert!(production.contains("document_cancel_identity("));
        assert!(production.contains("identity.scope_candidates()"));
        assert!(production.contains("identity.classify(Some(&receipt))"));
        assert!(production.contains("idempotency_key: &IdempotencyKey"));
        assert!(production.contains("idempotency_key: idempotency_key.clone()"));
        assert!(!production.contains("document_cancel_digest("));
        assert!(!production.contains("idempotency_key.to_string()"));
    }

    fn cancelled_instance() -> bpm::model::ApprovalProcessInstance {
        let mut instance = bpm::model::ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance-cancelled"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition-1"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adjustment-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("submitter-1").unwrap(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap();
        instance.cancel(Timestamp::from_unix_secs(2).unwrap()).unwrap();
        instance
    }

    #[test]
    fn cancel_audit_instance_prefix_has_unambiguous_boundaries() {
        let instance_id = "instance:1";
        let message = format!(
            "{}authority=runtime_admin reason=instance=8:spoofed ",
            cancel_audit_message_prefix(instance_id)
        );
        assert!(cancel_audit_matches_instance(&message, instance_id));
        assert!(!cancel_audit_matches_instance(&message, "instance"));
        assert!(!cancel_audit_matches_instance(&message, "instance:10"));
    }

    #[test]
    fn non_original_replay_uses_the_same_terminal_conflict_as_a_missing_key() {
        let instance = cancelled_instance();
        let missing_key_message = instance.cancellation_task_policy().unwrap_err().to_string();
        assert!(matches!(
            cancel_replay_actor_mismatch(&instance),
            Error::ConflictError(message) if message == missing_key_message
        ));
    }
}
