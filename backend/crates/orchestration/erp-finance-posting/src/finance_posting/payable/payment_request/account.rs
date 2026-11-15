//! 完整收款账号只交付有敏感揭示权限的当前申请任务责任人。
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_finance::dto::payable::{PaymentRecipientRevealView, RevealPaymentRecipientRequest};
use erp_party::{PartyExt, SensitiveDataCodec};
use erp_permissions::payable;
use erp_workflow::service::work_item::expected_task_version;
use erp_workflow::{WorkItemExt, WorkItemStatus, WorkItemType};
use persistence_core::Transactional;
use validator::Validate;

use super::*;
use crate::FinancePostingPorts;
use crate::finance_posting::task_access::{TaskPurpose, require_task};

impl PurchasePaymentRequestProcess {
    /// 在当前任务和敏感权限验证后主动揭示冻结收款账号。
    /// # 参数
    /// * `id` - 付款申请。
    /// * `req` - 当前任务及审核冻结账户的版本。
    /// * `actor` - 当前任务责任人。
    /// * `codec` - 共享敏感字段解码器。
    /// # 返回
    /// 返回完整账号并记录敏感揭示审计。
    /// # 错误
    /// 权限、当前任务、账户身份或来源变化时拒绝。
    pub async fn reveal_account(
        &self,
        id: &str,
        req: RevealPaymentRecipientRequest,
        actor: &AuditActor,
        codec: &SensitiveDataCodec,
    ) -> Result<PaymentRecipientRevealView> {
        req.validate()?;
        let task_version = expected_task_version(&req.expected_task_version)?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let codec = codec.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    reveal(
                        &db,
                        &rbac,
                        RevealInput { id: &id, req: &req, task_version, actor: &actor, codec: &codec },
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}

struct RevealInput<'a> {
    id: &'a str,
    req: &'a RevealPaymentRecipientRequest,
    task_version: u64,
    actor: &'a AuditActor,
    codec: &'a SensitiveDataCodec,
}

async fn reveal(
    db: &Database,
    rbac: &FinancePostingPorts,
    input: RevealInput<'_>,
    executor: &mut dyn Executor,
) -> Result<PaymentRecipientRevealView> {
    let RevealInput { id, req, task_version, actor, codec } = input;
    let authorization = &rbac.authorization;
    if !authorization.has_permission(actor, &payable::PAYMENT_RECIPIENT_REVEAL, executor).await? {
        return Err(Error::Forbidden("没有完整银行账号揭示权限".into()));
    }
    let request = load(db, id, executor).await?;
    authorize_reveal_task(db, rbac, &request, req, task_version, actor, executor).await?;
    source::revalidate(db, &request, executor).await?;
    let frozen = request
        .payee_account
        .as_ref()
        .ok_or_else(|| Error::ConflictError("申请缺少收款账户审核快照".into()))?;
    if frozen.account_id != req.expected_bank_account_id
        || frozen.version != req.expected_bank_account_version
    {
        return Err(Error::ConflictError("页面账户与申请审核快照不一致".into()));
    }
    let account = db
        .party_bank_accounts()
        .find_by_id(&frozen.account_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("收款账户不存在".into()))?;
    let account_number = codec.decrypt(&account.account_number_ciphertext)?;
    let audit = actor.clone().resource_log(
        "purchase_payment_request.account.reveal",
        "purchase_payment_request",
        id.to_owned(),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(PaymentRecipientRevealView {
        bank_account_id: account.base.id,
        account_number,
        account_name: account.account_name,
        bank_name: account.bank_name,
        bank_branch_name: account.bank_branch_name,
        bank_routing_number: account.bank_routing_number,
    })
}

async fn authorize_reveal_task(
    db: &Database,
    rbac: &FinancePostingPorts,
    request: &PurchasePaymentRequest,
    req: &RevealPaymentRecipientRequest,
    task_version: u64,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let task = db
        .work_items()
        .find_by_id(&req.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款申请任务不存在".into()))?;
    if task.base.version != task_version
        || task.business_object_type != "purchase_payment_request"
        || task.business_object_id != request.base.id
        || task.subject_version != request.submission_version.to_string()
        || task.status != WorkItemStatus::Open
    {
        return Err(Error::Forbidden("只有当前付款申请任务责任人可以查看收款账号".into()));
    }
    require_task(db, rbac, actor, &task, TaskPurpose::Reveal, executor).await?;
    match task.work_item_type {
        WorkItemType::SupplierPaymentExecution => {
            execution::authorize_loaded_task(
                rbac.tasks.as_ref(),
                request,
                &task,
                task_version,
                actor,
                executor,
            )
            .await?;
        },
        WorkItemType::DocumentApproval => {
            if request.status != PurchasePaymentRequestStatus::InApproval {
                return Err(Error::Forbidden("申请已结束财务审批，旧任务不得揭示账号".into()));
            }
            let service = rbac.tasks.as_ref();
            service.ensure_item_access(actor, &task, executor).await?;
        },
        _ => return Err(Error::Forbidden("当前任务不具备付款申请账户资格".into())),
    }
    Ok(())
}
