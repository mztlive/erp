//! 开票申请详情、列表及额度汇总；审批运行信息通过通用审批查询读取。
use std::collections::HashMap;

use erp_core::money::Amount;
use erp_finance::dto::receivable::{InvoiceRequestAmounts, InvoiceRequestQuery, PageView};
use erp_finance::entity::receivable::{CustomerReceiptStatus, InvoiceRequestStatus, SalesInvoiceRequest};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::BpmExt;
use persistence_core::{Executor, NoTransaction};
use serde::Serialize;

use super::ReceivableReadService;
use crate::approval_runtime::binding_route;
use crate::finance::dto::DocumentApprovalView;
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
    /// 分页读取申请；查询条件和总数保持一致。
    ///
    /// # 参数
    /// * `query` - 开票申请列表条件。
    ///
    /// # 返回
    /// 返回申请分页；列表不含审批运行详情。
    ///
    /// # 错误
    /// 仓储读取失败或来源销售单不存在时返回错误。
    pub async fn invoice_request_list(
        &self,
        query: &InvoiceRequestQuery,
    ) -> Result<PageView<InvoiceRequestView>> {
        let page = self.db.sales_invoice_requests().page(query, &mut NoTransaction).await?;
        let items = self.invoice_request_summaries(page.items).await?;
        Ok(PageView {
            items,
            total: page.total,
            page: query.page.unwrap_or(1).max(1),
            page_size: query.page_size.unwrap_or(20).clamp(1, 100),
        })
    }
    /// 读取申请及审批定义；不存在时返回 NotFound。
    ///
    /// # 参数
    /// * `id` - 开票申请身份。
    ///
    /// # 返回
    /// 返回含销售单号、申请人与审批结构的申请详情。
    ///
    /// # 错误
    /// 申请或来源销售单不存在时返回 `NotFound`；审批绑定、流程实例或账号读取失败时返回对应错误。
    pub async fn invoice_request_detail(&self, id: &str) -> Result<InvoiceRequestView> {
        self.invoice_request_detail_with_executor(id, &mut NoTransaction).await
    }

    /// 在调用方已授权事务内读取申请及审批详情，避免授权和响应读取跨快照。
    ///
    /// # 参数
    /// * `id` - 已授权申请身份。
    /// * `executor` - 当前命令或回放的同一事务。
    /// # 返回
    /// 返回同一快照内的申请详情。
    /// # 错误
    /// 申请、来源或审批资料缺失及仓储错误时拒绝。
    pub async fn invoice_request_detail_with_executor(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoiceRequestView> {
        let request = self
            .db
            .sales_invoice_requests()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        self.invoice_request_view(request, executor).await
    }
    /// 汇总本应收的审批中、批准待开与可申请额度。
    ///
    /// # 参数
    /// * `account_id` - 应收子账身份。
    ///
    /// # 返回
    /// 返回审批中、批准待开、可申请与已开票金额。
    ///
    /// # 错误
    /// 应收不存在或仓储失败时返回错误。
    pub async fn invoice_request_amounts(&self, account_id: &str) -> Result<InvoiceRequestAmounts> {
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(account_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售应收不存在".into()))?;
        let requests =
            self.db.sales_invoice_requests().reserved_for_account(account_id, &mut NoTransaction).await?;
        let mut pending = Amount::zero();
        let mut approved = Amount::zero();
        for request in requests {
            match request.status {
                InvoiceRequestStatus::InApproval => pending = pending.checked_add(request.remaining()),
                InvoiceRequestStatus::Approved => approved = approved.checked_add(request.remaining()),
                _ => {},
            }
        }
        let source = super::invoice_request_source::load(&self.db, &account, &mut NoTransaction).await?;
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
    /// 装配申请的真实销售单号与审批绑定，缺失来源时失败关闭。
    async fn invoice_request_view(
        &self,
        request: SalesInvoiceRequest,
        executor: &mut dyn Executor,
    ) -> Result<InvoiceRequestView> {
        let binding = erp_workflow::service::document_registry::find_approval_binding(
            &self.db,
            &request.base.id,
            executor,
        )
        .await?;
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
        let subject = erp_workflow::entity::approval_integration::subject_ref_for(
            erp_workflow::DocumentType::SalesInvoiceRequest,
            &request.base.id,
        )?;
        if let Some(instance) = self.db.bpm_workflow().find_latest_by_subject(&subject, executor).await? {
            let current = self
                .db
                .bpm_workflow()
                .find_current_execution(
                    &bpm::ApprovalProcessInstanceId::new(instance.base.id.clone()),
                    executor,
                )
                .await?;
            approval.instance = Some(
                crate::finance::dto::DocumentApprovalInstanceView::new(
                    instance.base.id,
                    instance.status.as_str().into(),
                )
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

    /// 批量补齐列表单号与申请人，审批运行详情仅在打开单据时读取。
    async fn invoice_request_summaries(
        &self,
        requests: Vec<SalesInvoiceRequest>,
    ) -> Result<Vec<InvoiceRequestView>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let sales_ids = requests.iter().map(|item| item.sales_order_id.to_string()).collect::<Vec<_>>();
        let actors = requests.iter().map(|item| item.created_by.clone()).collect::<Vec<_>>();
        let orders = self
            .db
            .sales_orders()
            .list_active_by_ids(&sales_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.order_no))
            .collect::<HashMap<_, _>>();
        let names = self
            .db
            .accounts()
            .list_work_item_party_accounts(&actors, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect::<HashMap<_, _>>();
        requests
            .into_iter()
            .map(|request| {
                let sales_order_no = orders
                    .get(request.sales_order_id.as_ref())
                    .cloned()
                    .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
                Ok(InvoiceRequestView {
                    created_by_name: names.get(&request.created_by).cloned(),
                    request,
                    sales_order_no,
                    approval: None,
                })
            })
            .collect()
    }
}

/// 把开票申请状态投影到共用审批动作策略，不改变原单据状态。
///
/// # 参数
/// * `status` - 当前开票申请业务状态。
/// # 返回
/// 草稿、驳回和审批中分别保留动作资格，其余状态禁止提交或撤回。
/// # 错误
/// 不返回错误。
fn approval_status(status: InvoiceRequestStatus) -> CustomerReceiptStatus {
    match status {
        InvoiceRequestStatus::Draft => CustomerReceiptStatus::Draft,
        InvoiceRequestStatus::Rejected => CustomerReceiptStatus::Rejected,
        InvoiceRequestStatus::InApproval => CustomerReceiptStatus::InApproval,
        _ => CustomerReceiptStatus::Posted,
    }
}
