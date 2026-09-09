//! 开票申请详情、列表及额度汇总；审批运行信息通过通用审批查询读取。
use super::ReceivableReadService;
use crate::finance::dto::DocumentApprovalView;
use crate::{Error, Result};
use erp_core::money::Amount;
use erp_finance::dto::receivable::{InvoiceRequestAmounts, InvoiceRequestQuery, PageView};
use erp_finance::entity::receivable::{InvoiceRequestStatus, SalesInvoiceRequest};
use erp_finance::repository::ReceivableExt;
use erp_identity::AccessControlExt;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::BpmExt;
use persistence_core::NoTransaction;
use serde::Serialize;
use std::collections::HashMap;

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
    /// # 错误
    /// 仓储、绑定或来源单据读取失败时返回错误。
    pub async fn invoice_request_list(
        &self,
        query: &InvoiceRequestQuery,
    ) -> Result<PageView<InvoiceRequestView>> {
        let page = self
            .db
            .sales_invoice_requests()
            .page(query, &mut NoTransaction)
            .await?;
        let items = self.invoice_request_summaries(page.items).await?;
        Ok(PageView {
            items,
            total: page.total,
            page: query.page.unwrap_or(1).max(1),
            page_size: query.page_size.unwrap_or(20).clamp(1, 100),
        })
    }
    /// 读取申请及审批定义；不存在时返回 NotFound。
    pub async fn invoice_request_detail(&self, id: &str) -> Result<InvoiceRequestView> {
        let request = self
            .db
            .sales_invoice_requests()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("开票申请不存在".into()))?;
        self.invoice_request_view(request).await
    }
    /// 汇总本应收的审批中、批准待开与可申请额度。
    /// # 错误
    /// 应收不存在或仓储失败时返回错误。
    pub async fn invoice_request_amounts(&self, account_id: &str) -> Result<InvoiceRequestAmounts> {
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(account_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售应收不存在".into()))?;
        let requests = self
            .db
            .sales_invoice_requests()
            .reserved_for_account(account_id, &mut NoTransaction)
            .await?;
        let mut pending = Amount::zero();
        let mut approved = Amount::zero();
        for request in requests {
            match request.status {
                InvoiceRequestStatus::InApproval => pending = pending.checked_add(request.remaining()),
                InvoiceRequestStatus::Approved => approved = approved.checked_add(request.remaining()),
                _ => {}
            }
        }
        Ok(InvoiceRequestAmounts {
            receivable_account_id: account_id.into(),
            available_amount: account
                .open_invoiceable_total
                .checked_sub(pending)
                .checked_sub(approved)
                .max(Amount::zero()),
            pending_amount: pending,
            approved_remaining_amount: approved,
            invoiced_amount: account.invoiced_total,
        })
    }
    /// 装配申请的真实销售单号与审批绑定，缺失来源时失败关闭。
    async fn invoice_request_view(&self, request: SalesInvoiceRequest) -> Result<InvoiceRequestView> {
        let binding = erp_workflow::service::document_registry::find_approval_binding(
            &self.db,
            &request.base.id,
            &mut NoTransaction,
        )
        .await?;
        let status = match request.status {
            InvoiceRequestStatus::Draft => erp_finance::entity::receivable::CustomerReceiptStatus::Draft,
            InvoiceRequestStatus::InApproval => {
                erp_finance::entity::receivable::CustomerReceiptStatus::InApproval
            }
            _ => erp_finance::entity::receivable::CustomerReceiptStatus::Posted,
        };
        let order = self
            .db
            .sales_orders()
            .find_by_id(&request.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
        let mut approval = super::approval_view::document_approval_view(binding.as_ref(), None, status);
        let subject = erp_workflow::entity::approval_integration::subject_ref_for(
            erp_workflow::DocumentType::SalesInvoiceRequest,
            &request.base.id,
        )?;
        if let Some(instance) = self
            .db
            .bpm_workflow()
            .find_latest_by_subject(&subject, &mut NoTransaction)
            .await?
        {
            let current = self
                .db
                .bpm_workflow()
                .find_current_execution(
                    &bpm::ApprovalProcessInstanceId::new(instance.base.id.clone()),
                    &mut NoTransaction,
                )
                .await?;
            approval.instance = Some(crate::finance::dto::DocumentApprovalInstanceView {
                id: instance.base.id,
                status: instance.status.as_str().into(),
                current_round_no: instance.current_round_no,
                current_node: current.as_ref().map(|e| e.node_name.clone()),
                current_assignee: current.as_ref().map(|e| e.assignee_name_snapshot.clone()),
                latest_rejection: None,
            });
        }
        let creator = self
            .db
            .accounts()
            .list_work_item_party_accounts(std::slice::from_ref(&request.created_by), &mut NoTransaction)
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
        let sales_ids = requests
            .iter()
            .map(|item| item.sales_order_id.to_string())
            .collect::<Vec<_>>();
        let actors = requests
            .iter()
            .map(|item| item.created_by.clone())
            .collect::<Vec<_>>();
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
