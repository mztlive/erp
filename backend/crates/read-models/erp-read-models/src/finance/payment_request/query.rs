//! 完整来源资格先于分页与整单金额，当前事实与冻结依据分别返回。

use application_core::{AuditActor, PageView};
use erp_finance::dto::payable::payment_request::PurchasePaymentRequestQuery;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestStatus};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_procurement::repository::PurchaseOrderExt;
use erp_workflow::dto::work_item::WorkItemAllowedAction;
use erp_workflow::{DocumentType, WorkItemExt, WorkItemStatus, WorkItemType};
use persistence_core::{Executor, Transactional};

use super::list::PaymentRequestList;
use super::previews::frozen_previews;
use super::{PaymentRequestCurrentLine, PurchasePaymentRequestReadService, PurchasePaymentRequestView};
use crate::approval_runtime::load_document_runtime_for_instance;
use crate::finance::dto::{DocumentApprovalHistoryPageView, DocumentApprovalView};
use crate::ports::work_item_authorization::{AuthorizedTaskFact, WorkItemAuthorizationReadPort};
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    /// 按全部采购来源过滤后分页，列表和总数共用同一读取事务。
    ///
    /// # 参数
    /// * `actor` / `query` - 当前账号与申请筛选。
    /// # 返回
    /// 返回整单申请分页，无部分来源泄漏。
    /// # 错误
    /// 动作拒绝、超过候选上限或读取失败时拒绝。
    pub async fn list(
        &self,
        actor: &AuditActor,
        query: &PurchasePaymentRequestQuery,
    ) -> Result<PageView<PurchasePaymentRequestView>> {
        let this = self.clone();
        let actor = actor.clone();
        let query = query.clone().normalize()?;
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.list_snapshot(&actor, &query, executor).await })
            })
            .await
    }

    /// 逐采购来源证明普通详情资格，不以申请人字段或供应商访问替代来源范围。
    ///
    /// # 参数
    /// * `actor` / `id` - 当前账号与申请身份。
    /// # 返回
    /// 返回完整冻结依据和当前余额进度。
    /// # 错误
    /// 任一来源不可读或申请不存在时均返回 NotFound。
    pub async fn detail(&self, actor: &AuditActor, id: &str) -> Result<PurchasePaymentRequestView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let request = this.load(&id, executor).await?;
                    let scope = this.source_scope(&actor, "detail", executor).await?;
                    if !this.readable(&request, &scope, executor).await? {
                        return Err(unavailable());
                    }
                    let allowed_actions = this.allowed_actions(&actor, &request, executor).await?;
                    let mut view = this.view(request, executor).await?;
                    view.allowed_actions = allowed_actions;
                    Ok::<_, Error>(view)
                })
            })
            .await
    }

    /// 当前审批人和执行人只凭专用任务资格读取本申请必要资料。
    ///
    /// # 参数
    /// * `actor` / `task_id` / `auth` - 当前身份、正式任务与工作流授权器。
    /// # 返回
    /// 返回同形状整单上下文，不要求全局销售、采购、合同或文件权限。
    /// # 错误
    /// 任务不匹配、失效、来源版本变化或责任资格改变时拒绝。
    pub async fn task_context(
        &self,
        actor: &AuditActor,
        task_id: &str,
        auth: &impl WorkItemAuthorizationReadPort,
    ) -> Result<PurchasePaymentRequestView> {
        let fact = auth.authorize(task_id, actor).await?;
        let this = self.clone();
        let id = task_id.to_owned();
        let expected = fact.clone();
        let actor_for_tx = actor.clone();
        let view = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let request = this.task_request(&actor_for_tx, &id, &expected, executor).await?;
                    let mut view = this.view(request, executor).await?;
                    if expected.work_item_type == WorkItemType::SupplierPaymentExecution
                        && expected.allowed_actions.contains(&WorkItemAllowedAction::Process)
                        && view.current_lines.iter().all(|line| !line.source_changed)
                    {
                        view.allowed_actions.push("PAY".into());
                    }
                    Ok::<_, Error>(view)
                })
            })
            .await?;
        if auth.authorize(task_id, actor).await? != fact {
            return Err(Error::ConflictError("付款申请任务资格已变化".into()));
        }
        Ok(view)
    }

    async fn list_snapshot(
        &self,
        actor: &AuditActor,
        query: &PurchasePaymentRequestQuery,
        executor: &mut dyn Executor,
    ) -> Result<PageView<PurchasePaymentRequestView>> {
        let scope = self.source_scope(actor, "list", executor).await?;
        let candidates = self.db.purchase_payment_requests().scope_candidates(query, executor).await?;
        if candidates.len() > 10_000 {
            return Err(Error::ValidationError("付款申请查询超过上限，请收窄筛选".into()));
        }
        let mut rows = Vec::new();
        for row in candidates {
            if self.readable(&row, &scope, executor).await? {
                rows.push(row);
            }
        }
        let mut applicant_ids: Vec<_> = rows.iter().map(|row| row.created_by.clone()).collect();
        applicant_ids.sort();
        applicant_ids.dedup();
        let names = self.db.accounts().names_by_ids(&applicant_ids, executor).await?;
        let page = PaymentRequestList::page(rows, &names, query)?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            items.push(self.view(row, executor).await?);
        }
        Ok(PageView { items, total: page.total, page: page.page, page_size: page.page_size })
    }

    pub(super) async fn load(&self, id: &str, executor: &mut dyn Executor) -> Result<PurchasePaymentRequest> {
        self.db.purchase_payment_requests().find_by_id(id, executor).await?.ok_or_else(unavailable)
    }

    pub(super) async fn task_request(
        &self,
        actor: &AuditActor,
        task_id: &str,
        fact: &AuthorizedTaskFact,
        executor: &mut dyn Executor,
    ) -> Result<PurchasePaymentRequest> {
        if fact.business_object_type != "purchase_payment_request"
            || !matches!(
                fact.work_item_type,
                WorkItemType::DocumentApproval | WorkItemType::SupplierPaymentExecution
            )
        {
            return Err(unavailable());
        }
        if !task_material_actionable(fact) {
            return Err(Error::Forbidden("仅当前审批人或付款任务责任人可读取任务材料".into()));
        }
        let task = self.db.work_items().find_by_id(task_id, executor).await?.ok_or_else(unavailable)?;
        let request = self.load(&fact.business_object_id, executor).await?;
        if task.status != WorkItemStatus::Open
            || task.work_item_type != fact.work_item_type
            || task.owner_user_id.as_deref() != Some(actor.id())
            || task.business_object_id != request.base.id
            || task.business_object_type != fact.business_object_type
            || task.subject_version != fact.subject_version
            || task.subject_version != request.submission_version.to_string()
            || (fact.work_item_type == WorkItemType::DocumentApproval
                && request.status != PurchasePaymentRequestStatus::InApproval)
            || (fact.work_item_type == WorkItemType::SupplierPaymentExecution
                && (request.work_item_id.as_deref() != Some(task_id)
                    || !matches!(
                        request.status,
                        PurchasePaymentRequestStatus::Approved | PurchasePaymentRequestStatus::PartiallyPaid
                    )))
        {
            return Err(Error::ConflictError("付款申请任务不属于当前提交版本".into()));
        }
        Ok(request)
    }

    async fn view(
        &self,
        request: PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<PurchasePaymentRequestView> {
        let (purchases, sales) = frozen_previews(&request.lines, &request.payer_party_id)?;
        let created_by_name = self
            .db
            .accounts()
            .names_by_ids(std::slice::from_ref(&request.created_by), executor)
            .await?
            .remove(&request.created_by);
        let supplier_name = request
            .lines
            .first()
            .and_then(|line| line.source_snapshot.purchase_preview.get("supplier_name"))
            .and_then(|value| value.as_str())
            .unwrap_or(&request.supplier_id)
            .to_string();
        let current_lines = self.current_lines(&request, executor).await?;
        let approval = self.approval(&request, executor).await?;
        let attachments = self.attachments(&request, executor).await?;
        let payments = self.payments(&request, executor).await?;
        let credit_uses =
            erp_finance::repository::supplier_credit::uses_by_request(&self.db, &request.base.id, executor)
                .await?;
        Ok(PurchasePaymentRequestView {
            request,
            supplier_name,
            created_by_name,
            purchase_previews: purchases,
            sales_previews: sales,
            current_lines,
            approval,
            allowed_actions: Vec::new(),
            attachments,
            payments,
            credit_uses,
        })
    }

    async fn current_lines(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentRequestCurrentLine>> {
        let mut lines = Vec::new();
        let bank_changed = self.bank_account_changed(request, executor).await?;
        for line in &request.lines {
            let account = self
                .db
                .payable_accounts()
                .find_by_id(&line.payable_account_id, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("付款申请来源应付不存在".into()))?;
            let order = self
                .db
                .purchase_orders()
                .find_by_id(&line.purchase_order_id, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("付款申请来源采购不存在".into()))?;
            lines.push(PaymentRequestCurrentLine {
                payable_entries: self.payable_entries(&line.payable_account_id, executor).await?,
                purchase_order_id: line.purchase_order_id.clone(),
                payable_account_id: line.payable_account_id.clone(),
                purchase_revision_id: order.stable.current_revision_id.clone(),
                status: order.stable.status.as_str().into(),
                receiving_progress: order.fulfillment_progress.as_str().into(),
                current_gross_total: account.gross_total,
                current_settled_total: account.settled_total,
                current_open_total: account.open_total,
                current_reserved_total: account.payment_request_reserved_total,
                current_available_total: account
                    .open_total
                    .checked_sub(account.payment_request_reserved_total),
                source_changed: bank_changed
                    || self.source_changed(request, line, &order, &account, executor).await?,
            });
        }
        Ok(lines)
    }

    async fn approval(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<DocumentApprovalView> {
        let binding = self.request_approval_binding(request, executor).await?;
        let mut view = DocumentApprovalView {
            requirement: "PROCESS_REQUIRED".into(),
            definition: self.approval_definition(binding.as_ref(), executor).await?,
            instance: None,
            recent_history: Vec::new(),
            history_page: DocumentApprovalHistoryPageView::default(),
            allowed_actions: Vec::new(),
        };
        match request.status {
            PurchasePaymentRequestStatus::Draft
            | PurchasePaymentRequestStatus::Rejected
            | PurchasePaymentRequestStatus::Withdrawn => view.allowed_actions.push("SUBMIT".into()),
            PurchasePaymentRequestStatus::InApproval => view.allowed_actions.push("CANCEL".into()),
            _ => {},
        }
        if let Some(instance_id) = request.approval_instance_id.as_deref() {
            let runtime = load_document_runtime_for_instance(
                &self.db,
                DocumentType::PurchasePaymentRequest,
                &request.base.id,
                binding.as_ref(),
                instance_id,
                request.submission_version,
                executor,
            )
            .await?;
            if !runtime.cancellable {
                view.allowed_actions.retain(|action| action != "CANCEL");
            }
            view.instance = Some(runtime.instance);
            view.recent_history = runtime.history.items.into_iter().map(Into::into).collect();
            view.history_page = DocumentApprovalHistoryPageView {
                next_cursor: runtime.history.next_cursor,
                has_more: runtime.history.has_more,
            };
        } else if request.submission_version != 0 {
            return Err(Error::ConflictError("付款申请缺少本次提交的审批实例".into()));
        }
        Ok(view)
    }
}

fn unavailable() -> Error {
    Error::NotFound("付款申请不存在或无权查看".into())
}

fn task_material_actionable(fact: &AuthorizedTaskFact) -> bool {
    match fact.work_item_type {
        WorkItemType::DocumentApproval => {
            fact.allowed_actions.contains(&WorkItemAllowedAction::Approve)
                || fact.allowed_actions.contains(&WorkItemAllowedAction::Reject)
        },
        WorkItemType::SupplierPaymentExecution => {
            fact.allowed_actions.contains(&WorkItemAllowedAction::Process)
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_materials_require_current_action_and_never_accept_view_or_management_only() {
        let mut fact = AuthorizedTaskFact {
            work_item_type: WorkItemType::DocumentApproval,
            business_object_type: "purchase_payment_request".into(),
            business_object_id: "request".into(),
            subject_version: "1".into(),
            allowed_actions: vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Reassign],
        };
        assert!(!task_material_actionable(&fact));
        fact.allowed_actions.push(WorkItemAllowedAction::Approve);
        assert!(task_material_actionable(&fact));
        fact.work_item_type = WorkItemType::SupplierPaymentExecution;
        assert!(!task_material_actionable(&fact));
        fact.allowed_actions = vec![WorkItemAllowedAction::Process];
        assert!(task_material_actionable(&fact));
        fact.work_item_type = WorkItemType::SalesInvoiceExecution;
        assert!(!task_material_actionable(&fact));
    }
}
