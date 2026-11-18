//! 开票申请详情、列表及额度汇总；审批运行信息通过通用审批查询读取。
use std::sync::Arc;

use application_core::AuditActor;
use bpm::ApprovalProcessInstanceId;
use erp_core::money::Amount;
use erp_finance::dto::receivable::InvoiceRequestAmounts;
use erp_finance::entity::receivable::{CustomerReceiptStatus, InvoiceRequestStatus, SalesInvoiceRequest};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, AccessTarget, AuthorizationService};
use erp_permissions::internal;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::entity::approval_integration::subject_ref_for;
use erp_workflow::service::document_registry::find_approval_binding;
use erp_workflow::{BpmExt, DocumentType};
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::ReceivableReadService;
use crate::approval_runtime::binding_route;
use crate::finance::command_snapshot;
use crate::finance::dto::{DocumentApprovalInstanceView, DocumentApprovalView};
use crate::{Error, Result};

/// 申请详情包含固定来源、单据审批绑定和中文业务单号。
#[derive(Debug, Clone, Serialize)]
pub struct InvoiceRequestView {
    #[serde(flatten)]
    pub request: SalesInvoiceRequest,
    pub sales_order_no: String,
    pub created_by_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<DocumentApprovalView>,
}
impl ReceivableReadService {
    /// 读取开票申请时，在调用方已授权事务内读取申请及审批详情，避免授权和响应读取跨快照。
    ///
    /// # 参数
    /// * `id` - 已授权申请身份。
    /// * `executor` - 当前命令或回放的同一事务。
    ///
    /// # 返回
    /// 返回同一快照内的申请详情。
    ///
    /// # 错误
    /// 非活动快照、申请、来源或审批资料缺失及仓储错误时拒绝。
    pub async fn invoice_request_detail_with_executor(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoiceRequestView> {
        command_snapshot(executor)?;
        let request = self
            .db
            .sales_invoice_requests()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        self.invoice_request_view(request, executor).await
    }
    /// 计算开票申请金额时，汇总本应收的审批中、批准待开与可申请额度。
    ///
    /// # 参数
    /// * `account_id` - 应收子账身份。
    /// * `authorization` - 组合根共享的统一授权服务。
    ///
    /// # 返回
    /// 返回审批中、批准待开、可申请与已开票金额。
    ///
    /// # 错误
    /// 应收不存在或仓储失败时返回错误。
    pub async fn invoice_request_amounts(
        &self,
        account_id: &str,
        actor: &AuditActor,
        authorization: Arc<AuthorizationService>,
    ) -> Result<InvoiceRequestAmounts> {
        let (this, actor, account_id) = (Self::new(self.db.clone()), actor.clone(), account_id.to_owned());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { this.amounts_snapshot(&account_id, &actor, &authorization, executor).await },
                )
            })
            .await
    }

    // 计算开票申请金额时，额度读取只使用既有开票申请列表功能资格，所有来源资料共用原快照。
    async fn amounts_snapshot(
        &self,
        account_id: &str,
        actor: &AuditActor,
        authorization: &AuthorizationService,
        executor: &mut dyn Executor,
    ) -> Result<InvoiceRequestAmounts> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError(
                "暂时无法查询可开票金额，请从销售单详情重新打开开票申请".into(),
            ));
        }
        authorization
            .require(actor, &internal::INVOICE_REQUEST_AMOUNTS_READ, AccessTarget::Operation, executor)
            .await?;

        let boundary = executor.boundary().ok_or_else(|| {
            Error::ValidationError("暂时无法确认可查看的开票金额，请从销售单详情重新打开开票申请".into())
        })?;
        let view = self.amounts_view(account_id, executor).await?;
        if !boundary.is_current(executor) {
            return Err(Error::ValidationError("本次开票金额查询已失效，请刷新销售单后重新查看".into()));
        }
        Ok(view)
    }

    // 生成开票申请视图时，组合原用例的额度、抬头和税号，不执行额外菜单或对象资格判断。
    async fn amounts_view(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoiceRequestAmounts> {
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(account_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售应收不存在".into()))?;

        let requests = self.db.sales_invoice_requests().reserved_for_account(account_id, executor).await?;
        let mut pending = Amount::zero();
        let mut approved = Amount::zero();
        for request in requests {
            match request.status {
                InvoiceRequestStatus::InApproval => pending = pending.checked_add(request.remaining()),
                InvoiceRequestStatus::Approved => approved = approved.checked_add(request.remaining()),
                _ => {},
            }
        }

        let source = super::invoice_request_source::load(&self.db, &account, executor).await?;
        let available =
            account.open_invoiceable_total.checked_sub(pending).checked_sub(approved).max(Amount::zero());
        let unavailable_reason = source.unavailable_reason(available).map(str::to_owned);

        Ok(InvoiceRequestAmounts {
            can_submit: unavailable_reason.is_none(),
            unavailable_reason,
            invoice_title: source.invoice_title,
            tax_number: source.tax_number,
            receivable_account_id: account_id.into(),
            available_amount: available,
            pending_amount: pending,
            approved_remaining_amount: approved,
            invoiced_amount: account.invoiced_total,
        })
    }
    // 生成开票申请视图时，组合申请的真实销售单号与审批绑定，缺失来源时失败关闭。
    async fn invoice_request_view(
        &self,
        request: SalesInvoiceRequest,
        executor: &mut dyn Executor,
    ) -> Result<InvoiceRequestView> {
        let binding = find_approval_binding(&self.db, &request.base.id, executor).await?;
        let status = approval_status(request.status);
        let order = self
            .db
            .sales_orders()
            .find_by_id(&request.sales_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;

        let mut approval = super::approval_view::document_approval_view(binding.as_ref(), None, status);
        if let (Some(binding), Some(definition)) = (binding.as_ref(), approval.definition.as_mut()) {
            let (name, nodes) = binding_route(&self.db, binding, executor).await?;
            definition.name = name;
            definition.nodes = nodes;
        }

        let subject = subject_ref_for(DocumentType::SalesInvoiceRequest, &request.base.id)?;
        if let Some(instance) = self.db.bpm_workflow().find_latest_by_subject(&subject, executor).await? {
            let current = self
                .db
                .bpm_workflow()
                .find_current_execution(&ApprovalProcessInstanceId::new(instance.base.id.clone()), executor)
                .await?;
            approval.instance = Some(
                DocumentApprovalInstanceView::new(instance.base.id, instance.status.as_str().into())
                    .with_current_round_no(instance.current_round_no)
                    .with_current_node(current.as_ref().map(|e| e.node_name.clone()))
                    .with_current_assignee(current.as_ref().map(|e| e.assignee_name_snapshot.clone())),
            );
        }

        let creator = self
            .db
            .accounts()
            .list_work_item_party_accounts(std::slice::from_ref(&request.created_by), executor)
            .await?
            .into_iter()
            .next();

        Ok(InvoiceRequestView {
            request,
            sales_order_no: order.order_no,
            created_by_name: creator.map(|creator| creator.name),
            approval: Some(approval),
        })
    }
}

// 将开票申请状态投影到共用审批动作策略，不改变原单据状态。
//
// 调用参数：
// * `status` - 当前开票申请业务状态。
//
// 草稿、驳回和审批中分别保留动作资格，其余状态禁止提交或撤回。
fn approval_status(status: InvoiceRequestStatus) -> CustomerReceiptStatus {
    match status {
        InvoiceRequestStatus::Draft => CustomerReceiptStatus::Draft,
        InvoiceRequestStatus::Rejected => CustomerReceiptStatus::Rejected,
        InvoiceRequestStatus::InApproval => CustomerReceiptStatus::InApproval,
        _ => CustomerReceiptStatus::Posted,
    }
}

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn amounts_require_snapshot_and_list_qualification_before_financial_reads() {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("invoice_request_amounts_authorization");
        let service = ReceivableReadService::new(db);
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let authorization = unavailable_authorization();

        let result =
            service.amounts_snapshot("account", &actor, &authorization, &mut ReadExecutor::default()).await;
        assert!(
            matches!(result, Err(Error::ValidationError(reason)) if reason == "暂时无法查询可开票金额，请从销售单详情重新打开开票申请")
        );

        let result = service
            .amounts_snapshot("account", &actor, &authorization, &mut SnapshotExecutor::default())
            .await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }
}
