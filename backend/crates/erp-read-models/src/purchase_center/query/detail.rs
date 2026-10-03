//! 采购详情有序读取与纯视图装配；调用方继续负责读取前后的采购授权重验。

use erp_procurement::dto::purchase_order::{
    PurchaseChangeSummaryView, PurchaseOrderLineView, PurchaseSalesAllocationView, TotalsView,
};
use erp_procurement::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseLineSalesAllocation, PurchaseOrder,
};
use erp_procurement::service::purchase_order::view_mapping::{
    revision_line_to_view, revision_totals, submission_line_to_view,
};
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::NoTransaction;

use super::{center_content_source, owner_display, sales_no_for, supplier_display};
use crate::purchase_center::PurchaseOrderReadService;
use crate::purchase_center::approval_query::load_document_approval;
use crate::purchase_center::dto::{
    DocumentApprovalView, PurchaseOrderCenterView, PurchaseOrderPayableSummaryView,
    PurchaseSourceSalesOrderView,
};
use crate::purchase_center::repository::{PurchaseOrderCenterFacts, load_purchase_order_center_facts};
use crate::purchase_center::source_sales::source_sales;
use crate::{Error, Result};

/// 当前采购事实中已经按原有校验顺序解析的展示身份。
struct CenterIdentity {
    supplier_name: String,
    sales_order_id: String,
    sales_order_no: String,
    owner_user_id: String,
    owner_name: String,
}

/// 当前内容选择结果；版本优先于提交，缺内容时保持零金额和空行。
struct CenterContent {
    source: String,
    lines: Vec<PurchaseOrderLineView>,
    totals: TotalsView,
}

/// 已读取的分配、变更和财务事实，只执行无 I/O 的映射。
struct CenterRelations {
    revision_no: Option<u32>,
    allocations: Vec<PurchaseSalesAllocationView>,
    changes: Vec<PurchaseChangeSummaryView>,
    payable_summary: Option<PurchaseOrderPayableSummaryView>,
}

impl PurchaseOrderReadService {
    /// 按原有顺序装配已授权采购对象中心；本方法不替代入口前后的授权重验。
    ///
    /// # 参数
    /// * `id` - 调用方已经证明采购详情资格的采购单身份。
    /// # 返回
    /// 返回原采购事实、可选关联销售投影及审批结构。
    /// # 错误
    /// 采购主表缺失、销售编号或责任关系异常、审批关联或仓储读取失败时拒绝。
    pub(super) async fn detail_view(&self, id: &str) -> Result<PurchaseOrderCenterView> {
        let mut facts = load_purchase_order_center_facts(&self.db, id, &mut NoTransaction).await?;
        let order = facts.order.take().ok_or_else(|| Error::NotFound("采购单不存在或无权查看".into()))?;
        let identity = center_identity(&order, &facts)?;
        let content = center_content(&facts);
        let source = match source_sales(&self.db, &order, &content.lines, &mut NoTransaction).await {
            Ok(source) => Some(source),
            Err(error) => {
                tracing::warn!(purchase_order_id = %id, error = %error, "采购关联销售资料暂不可读取");
                None
            },
        };
        let relations = center_relations(facts);
        let binding = match find_approval_binding(&self.db, &order.base.id, &mut NoTransaction)
            .await
            .map_err(Error::from)
        {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let approval =
            load_document_approval(&self.db, &order.base.id, binding.as_ref(), order.stable.status).await?;
        Ok(center_view(&order, identity, content, source, relations, approval))
    }
}

/// 保留供应商、销售单号、当前责任人的原校验顺序与默认展示规则。
fn center_identity(order: &PurchaseOrder, facts: &PurchaseOrderCenterFacts) -> Result<CenterIdentity> {
    let supplier_name = supplier_display(
        order.supplier_id.as_ref(),
        &facts.supplier_name.clone().map(|name| (order.supplier_id.to_string(), name)).into_iter().collect(),
    );
    let sales_order_id = order.sales_order_id.to_string();
    let sales_order_no = sales_no_for(
        &sales_order_id,
        &facts.sales_order_no.clone().map(|no| (sales_order_id.clone(), no)).into_iter().collect(),
    )?;
    let owner_user_id = order.current_owner_user_id()?.to_string();
    let (_, owner_name) = owner_display(
        Some(owner_user_id.clone()),
        &facts.owner_name.clone().map(|name| (owner_user_id.clone(), name)).into_iter().collect(),
    );
    Ok(CenterIdentity { supplier_name, sales_order_id, sales_order_no, owner_user_id, owner_name })
}

/// 当前版本 > 当前提交 > 空内容；沿用取消审批草稿的既有内容来源解释。
fn center_content(facts: &PurchaseOrderCenterFacts) -> CenterContent {
    let source = center_content_source(
        facts.current_revision.is_some(),
        facts.current_submission.as_ref().map(|submission| submission.content_source()),
    );
    let (lines, totals) = if let Some(revision) = &facts.current_revision {
        (facts.revision_lines.iter().map(revision_line_to_view).collect(), revision_totals(revision))
    } else if let Some(submission) = &facts.current_submission {
        (
            facts.submission_lines.iter().map(submission_line_to_view).collect(),
            TotalsView {
                gross: submission.gross_amount.to_string(),
                net: submission.net_amount.to_string(),
                tax: submission.tax_amount.to_string(),
            },
        )
    } else {
        (Vec::new(), TotalsView { gross: "0.00".into(), net: "0.00".into(), tax: "0.00".into() })
    };
    CenterContent { source, lines, totals }
}

/// 原始事实消费顺序保持分配、变更、版本号及应付汇总。
fn center_relations(facts: PurchaseOrderCenterFacts) -> CenterRelations {
    let allocations = allocation_views(facts.allocations);
    let changes = change_views(facts.changes);
    let revision_no = facts.current_revision.as_ref().map(|revision| revision.revision.revision_no);
    let payable_summary = facts.payable.as_ref().map(|account| PurchaseOrderPayableSummaryView {
        payable_open_amount: account.open_total,
        paid_allocated_amount: account.settled_total,
        purchase_invoice_allocated_amount: account.invoiced_total,
    });
    CenterRelations { revision_no, allocations, changes, payable_summary }
}

/// 生效采购销售分配逐字段转换，保留输入次序及所有精确十进制金额。
fn allocation_views(allocations: Vec<PurchaseLineSalesAllocation>) -> Vec<PurchaseSalesAllocationView> {
    allocations
        .into_iter()
        .map(|allocation| PurchaseSalesAllocationView {
            id: allocation.base.id,
            purchase_order_revision_line_id: allocation.purchase_order_revision_line_id.to_string(),
            sales_order_revision_line_id: allocation.sales_order_revision_line_id.to_string(),
            allocated_quantity: allocation.allocated_quantity.to_string(),
            allocated_cost_gross: allocation.allocated_cost_gross.to_string(),
            allocated_cost_net: allocation.allocated_cost_net.to_string(),
        })
        .collect()
}

/// 采购变更摘要逐字段转换，不更改排序或状态映射。
fn change_views(changes: Vec<PurchaseChangeOrder>) -> Vec<PurchaseChangeSummaryView> {
    changes
        .into_iter()
        .map(|change| PurchaseChangeSummaryView {
            change_id: change.base.id.clone(),
            status: change.stable.status.as_str().to_string(),
            base_revision_id: change.base_revision_id.to_string(),
            effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
            reason: change.reason,
            created_at: change.base.created_at,
        })
        .collect()
}

/// 已解析身份、内容及关联事实统一组装为既有对象中心响应。
fn center_view(
    order: &PurchaseOrder,
    identity: CenterIdentity,
    content: CenterContent,
    source_sales_order: Option<PurchaseSourceSalesOrderView>,
    relations: CenterRelations,
    approval: DocumentApprovalView,
) -> PurchaseOrderCenterView {
    PurchaseOrderCenterView {
        id: order.base.id.clone(),
        purchase_no: order.purchase_no.clone(),
        status: order.stable.status,
        review_status: order.review_status,
        version: order.base.version,
        sales_order_id: identity.sales_order_id,
        sales_order_no: identity.sales_order_no,
        supplier_id: order.supplier_id.to_string(),
        supplier_name: identity.supplier_name,
        purchase_type: order.purchase_type,
        payment_term_code: order.payment_term_code.clone(),
        fulfillment_responsibility: order.fulfillment_responsibility,
        owner_user_id: identity.owner_user_id,
        owner_name: identity.owner_name,
        target_warehouse_id: order.target_warehouse_id.as_ref().map(ToString::to_string),
        payment_progress: order.payment_progress,
        invoice_progress: order.invoice_progress,
        fulfillment_progress: order.fulfillment_progress,
        current_submission_id: order.current_submission_id.clone(),
        current_revision_id: order.stable.current_revision_id.clone(),
        revision_no: relations.revision_no,
        content_source: content.source,
        lines: content.lines,
        source_sales_order,
        totals: content.totals,
        allocations: relations.allocations,
        changes: relations.changes,
        payable_summary: relations.payable_summary,
        approval,
        created_at: order.base.created_at,
    }
}
