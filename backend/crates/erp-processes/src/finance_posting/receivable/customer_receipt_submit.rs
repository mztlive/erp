//! 原回款提交的完整载荷收据、当前资格回放与原子启动事务。

use application_core::{AuditActor, CommandReceipt, CommandReceiptFact};
use erp_audit::{AuditExt, AuditLog, AuditLogRepositoryExt as _, CommandReceiptServiceExt as _};
use erp_finance::dto::receivable::SubmitCustomerReceiptRequest;
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_identity::SharedRbacService;
use erp_read_models::finance::dto::CustomerReceiptView;
use erp_workflow::service::approval::execution::{PreparedExecution, command_recovery_delay};
use erp_workflow::service::document_registry::find_approval_binding;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::ReceivableProcess;
use super::adapter::{
    self, RECENT_HISTORY_LIMIT, build_customer_receipt_snapshot, customer_receipt_object_readable,
    customer_receipt_responsible_org_id, customer_receipt_start_command, customer_receipt_subject_ref,
    require_frozen_binding, start_approval_command_kind,
};
use super::customer_receipt_posting::prepare_dispatch_start;
use super::draft_read::{ensure_receipt_owner, receipt_view_with_actions};
use super::draft_update::ensure_receipt_edit_authorized;
use super::start_approval::{CustomerReceiptStartPersistInput, persist_customer_receipt_start_apply};
use crate::{Error, Result};

/// 原回款主键、版本、幂等键与完整拟核销分配共同构成提交载荷。
pub(super) fn submit_receipt(
    id: &str,
    req: &SubmitCustomerReceiptRequest,
    actor_id: &str,
) -> Result<CommandReceipt> {
    Ok(CommandReceipt::from_payload(
        "customer-receipt-submit-",
        actor_id,
        "customer_receipt.submit",
        "customer_receipt",
        &req.idempotency_key,
        &(id, req),
    )?)
}

impl ReceivableProcess {
    /// 按原顺序冻结提交快照和绑定启动计划，写入留在调用方事务。
    pub(super) async fn prepare_receipt_submit_input(
        &self,
        id: &str,
        receipt: CustomerReceipt,
        idempotency_key: &str,
        actor: &AuditActor,
        adapter: adapter::CustomerReceiptAdapter,
    ) -> Result<CustomerReceiptStartPersistInput> {
        let subject = customer_receipt_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = erp_core::common::time::Instant::now();
        let snapshot_payload = build_customer_receipt_snapshot(&receipt, actor.id(), now)?;
        let start =
            customer_receipt_start_command(id, receipt.approval_subject_version, actor.id(), idempotency_key);
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = customer_receipt_responsible_org_id(&receipt)?;
        let _ = customer_receipt_object_readable(&organization_id, actor.id())?;
        let prepared = prepare_dispatch_start(
            &self.db,
            &binding,
            &subject,
            receipt.approval_subject_version,
            &organization_id,
            actor.id(),
            idempotency_key,
            now,
        )
        .await?;
        Ok(CustomerReceiptStartPersistInput {
            receipt,
            actor: actor.clone(),
            id: id.to_owned(),
            snapshot_payload,
            prepared,
            owner_role: adapter.owner_role,
            organization_id,
            now,
        })
    }

    /// 在状态和版本门禁前，以当前身份、完整来源和原登记人回放同载荷提交。
    pub(super) async fn replay_customer_receipt_submit(
        &self,
        id: &str,
        command: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<CustomerReceiptView>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let command = command.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorized_receipt(&db, &rbac, &actor, &id, executor).await?;
                    if !committed_submit(&db, &command, &id, executor).await? {
                        return Ok(None);
                    }
                    Ok(Some(receipt_view_with_actions(&db, &rbac, &actor, &id, executor).await?))
                })
            })
            .await
    }

    /// 提交结果未知时只用完整业务收据恢复，每次 fresh 快照都重验当前资格。
    pub(super) async fn recover_customer_receipt_submit(
        &self,
        id: &str,
        command: &CommandReceipt,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<CustomerReceiptView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            match self.replay_customer_receipt_submit(id, command, actor).await {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {},
                Err(error) if error.command_may_have_committed() => {},
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }
}

/// 启动事实、完整提交收据和成功审计同事务写入；竞争重试只回读既有原单。
pub(super) async fn persist_receipt_submit(
    db: &Database,
    rbac: &SharedRbacService,
    input: CustomerReceiptStartPersistInput,
    command: CommandReceipt,
) -> Result<CustomerReceipt> {
    let db = db.clone();
    let rbac = rbac.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                let current = authorized_receipt(&db, &rbac, &input.actor, &input.id, executor).await?;
                if committed_submit(&db, &command, &input.id, executor).await? {
                    return Ok(current);
                }
                if !matches!(&input.prepared, PreparedExecution::Apply(_)) {
                    return Err(Error::ConflictError(
                        "历史回款启动缺少完整提交收据，不能确认本次载荷".into(),
                    ));
                }
                current.ensure_draft_editor(input.actor.id())?;
                ensure_expected_version(current.base.version, input.receipt.base.version)?;
                let audit = command.audit(input.actor.clone(), input.id.clone())?;
                let receipt = persist_customer_receipt_start_apply(&db, input, executor).await?;
                db.audit_logs().create(&audit, executor).await?;
                Ok(receipt)
            })
        })
        .await
}

/// 资格和原登记人事实必须在当前执行器内重新取得，不能沿用缓存授权。
async fn authorized_receipt(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<CustomerReceipt> {
    ensure_receipt_edit_authorized(db, rbac, actor, id, executor).await?;
    let receipt = db
        .customer_receipts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
    ensure_receipt_owner(&receipt, actor.id())?;
    Ok(receipt)
}

/// 使用相同执行器核验完整命令事实，并拒绝跨原单或损坏收据。
async fn committed_submit(
    db: &Database,
    command: &CommandReceipt,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let candidates = command.id_candidates();
    let facts = db.audit_logs().find_command_receipts_by_ids(&candidates, executor).await?;
    matches_committed_submit(command, id, &facts)
}

/// 完整命令匹配不能接受同键异载荷，也不能指向其他原单。
fn matches_committed_submit(
    command: &CommandReceipt,
    id: &str,
    facts: &[CommandReceiptFact],
) -> Result<bool> {
    let candidates = command.id_candidates();
    match AuditLog::pick_committed_resource_id(command, &candidates, facts)? {
        Some(committed_id) if committed_id == id => Ok(true),
        Some(_) => Err(Error::ConflictError("回款提交收据与原单不一致".into())),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::ReceivableEntryId;
    use erp_finance::dto::receivable::ReceiptAllocationLineRequest;

    use super::*;

    /// 同一操作重试可精确恢复；改变版本、来源、核销金额或原单均不能回放。
    #[test]
    fn submit_receipt_requires_the_complete_original_payload() {
        let request = SubmitCustomerReceiptRequest {
            expected_version: 2,
            idempotency_key: "retry".into(),
            allocations: vec![ReceiptAllocationLineRequest {
                receivable_entry_id: ReceivableEntryId::new("entry"),
                allocated_amount: "50.00".parse().unwrap(),
            }],
        };
        let command = submit_receipt("receipt", &request, "creator").unwrap();
        let fact =
            CommandReceiptFact::new(command.id(), "creator", "customer_receipt.submit", "customer_receipt")
                .with_resource_id("receipt")
                .with_success(true)
                .with_message(command.message(None));
        let replay = submit_receipt("receipt", &request, "creator").unwrap();
        assert!(matches_committed_submit(&replay, "receipt", std::slice::from_ref(&fact)).unwrap());
        assert!(!matches_committed_submit(&replay, "receipt", &[]).unwrap());
        assert!(matches_committed_submit(&replay, "other", std::slice::from_ref(&fact)).is_err());
        let mut changed_version = request.clone();
        changed_version.expected_version += 1;
        let mut changed_amount = request.clone();
        changed_amount.allocations[0].allocated_amount = "40.00".parse().unwrap();
        let mut changed_source = request.clone();
        changed_source.allocations[0].receivable_entry_id = ReceivableEntryId::new("other");
        for changed in [changed_version, changed_amount, changed_source] {
            let command = submit_receipt("receipt", &changed, "creator").unwrap();
            assert!(matches_committed_submit(&command, "receipt", std::slice::from_ref(&fact)).is_err());
        }
        let other_order = submit_receipt("other", &request, "creator").unwrap();
        assert!(matches_committed_submit(&other_order, "other", std::slice::from_ref(&fact)).is_err());
        let other_actor = submit_receipt("receipt", &request, "other").unwrap();
        assert!(
            other_actor.match_fact(&fact)
                != application_core::CommandReceiptMatch::SamePayload("receipt".into())
        );
    }
}
