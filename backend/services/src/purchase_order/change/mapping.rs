use database::{NoTransaction, PurchaseOrderExt, SalesOrderExt};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::ids::PurchaseChangeSubmissionId;
use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionData, PurchaseOrder,
    PurchaseOrderRevision,
};
use id_generator::next_id;

use super::super::change_adapter::document_approval_view;
use super::super::dto::{PurchaseChangeOrderView, SavePurchaseOrderLine, SubmitPurchaseChangeRequest};
use super::super::line_input::{compute_request_totals, to_line_inputs};
use super::super::PurchaseOrderService;
use crate::errors::{Error, Result};

impl PurchaseOrderService {
    /// 从变更单冻结的基准采购版本恢复完整目标行。
    ///
    /// # 参数
    /// * `revision_id` - 变更单冻结的采购生效版本稳定身份
    ///
    /// # 返回
    /// 返回按版本行号和稳定主键排序的完整目标行请求。
    ///
    /// # 错误
    /// 仓储读取失败或基准版本没有明细时返回错误。
    ///
    /// # 关键约束
    /// 排序由单版本仓储查询保证；空版本校验和 DTO 转换仍由 Service 负责。
    pub(super) async fn change_lines_from_base_revision(
        &self,
        revision_id: &entities::ids::PurchaseOrderRevisionId,
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        let lines = self
            .db
            .purchase_order()
            .list_revision_lines(revision_id, &mut NoTransaction)
            .await?;
        if lines.is_empty() {
            return Err(Error::BusinessLogicError("采购变更基准版本缺少明细".to_string()));
        }
        Ok(lines
            .into_iter()
            .map(|line| {
                let is_item = line.line_type == entities::purchase_order::PurchaseLineType::ItemService;
                SavePurchaseOrderLine {
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line
                        .procurement_confirmation_line_id
                        .map(|value| value.to_string()),
                    sku_id: line.sku_id.map(|value| value.to_string()),
                    sku_revision_id: line.sku_revision_id.map(|value| value.to_string()),
                    product_name: line.product_name_snapshot,
                    specification: line.specification_snapshot,
                    quantity: line.quantity.map(|value| value.to_string()),
                    base_unit_code: line.base_unit_code,
                    unit_cost_gross: line.unit_cost_gross.map(|value| value.to_string()),
                    input_tax_rate: line.input_tax_rate.map(|value| value.to_string()),
                    expected_delivery_date: line.expected_delivery_date.map(|value| value.to_string()),
                    sales_order_line_id: line.sales_order_line_id.map(|value| value.to_string()),
                    sales_order_revision_line_id: line
                        .sales_order_revision_line_id
                        .map(|value| value.to_string()),
                    sales_order_submission_line_id: None,
                    allocated_quantity: line.allocated_quantity.map(|value| value.to_string()),
                    gross_amount: if is_item {
                        None
                    } else {
                        Some(line.gross_amount.to_string())
                    },
                }
            })
            .collect())
    }

    /// 将采购变更目标行绑定到来源销售单当前版本行。
    ///
    /// # 参数
    /// * `order` - 原采购单，用于定位来源销售单
    /// * `lines` - 变更目标完整行请求
    ///
    /// # 返回
    /// 返回稳定销售行与销售当前版本行均已刷新的目标行。
    ///
    /// # 错误
    /// 来源销售单、当前销售版本或稳定销售行缺失，以及仓储查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 不再沿历史采购提交反查销售提交行；分配数量固定等于变更后的采购数量。
    pub(super) async fn enrich_change_lines_with_current_sales_revision(
        &self,
        order: &PurchaseOrder,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let revision_id = sales_order
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前版本".to_string()))?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(
                &entities::ids::SalesOrderRevisionId::new(revision_id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let by_stable_id = revision_lines
            .into_iter()
            .map(|line| (line.sales_order_line_id.to_string(), line))
            .collect::<std::collections::HashMap<_, _>>();
        enrich_change_lines(lines, &by_stable_id)
    }

    /// 构建变更提交（表头取自目标内容，提交动作由调用方冻结审计人）。
    pub(super) async fn build_change_submission(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        base_revision: &PurchaseOrderRevision,
        _supplier_name: &str,
        req: &SubmitPurchaseChangeRequest,
    ) -> Result<PurchaseChangeSubmission> {
        let inputs = to_line_inputs(&req.lines)?;
        let (gross, net, tax) = compute_request_totals(&inputs)?;
        let payment_term_code = req
            .payment_term_code
            .clone()
            .unwrap_or_else(|| base_revision.payment_term_snapshot.payment_term_code.clone());
        let payment_term_snapshot = self.payment_term_snapshot(&payment_term_code).await?;
        let next_no = self.next_change_submission_no(change).await?;
        PurchaseChangeSubmission::new(
            PurchaseChangeSubmissionId::new(next_id()),
            PurchaseChangeSubmissionData {
                purchase_change_order_id: change.base.id.clone().into(),
                submission_no: next_no,
                base_revision_id: change.base_revision_id.clone(),
                supplier_id: order.supplier_id.clone(),
                purchase_type: order.purchase_type,
                fulfillment_responsibility: order.fulfillment_responsibility,
                supplier_revision_id: base_revision.supplier_revision_id.clone(),
                supplier_snapshot: base_revision.supplier_snapshot.clone(),
                payment_term_snapshot,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )
        .map_err(Into::into)
    }

    /// 计算下一个变更提交序号。
    async fn next_change_submission_no(&self, change: &PurchaseChangeOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_order()
            .list_change_submissions_by_order(&change.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseChangeSubmission::next_submission_no(&existing).map_err(Into::into)
    }
}

/// 使用销售当前版本稳定行映射刷新采购变更请求行。
///
/// # 参数
/// * `lines` - 采购变更目标行
/// * `sales_lines` - 稳定销售行到当前销售版本行的映射
///
/// # 返回
/// 返回当前版本销售关联和分配数量已规范化的行。
///
/// # 错误
/// 商品行缺少稳定销售行、数量或当前销售版本对应行时返回一致性错误。
///
/// # 关键业务约束
/// 商品行 `allocated_quantity` 恒等于变更后的 `quantity`，物流行清空销售关联。
fn enrich_change_lines(
    lines: &[SavePurchaseOrderLine],
    sales_lines: &std::collections::HashMap<String, entities::sales_order::SalesOrderRevisionLine>,
) -> Result<Vec<SavePurchaseOrderLine>> {
    let mut enriched = lines.to_vec();
    for line in &mut enriched {
        if line.line_type == entities::purchase_order::PurchaseLineType::LogisticsFee {
            line.sales_order_line_id = None;
            line.sales_order_revision_line_id = None;
            line.sales_order_submission_line_id = None;
            line.allocated_quantity = None;
            continue;
        }
        let stable_id = line
            .sales_order_line_id
            .clone()
            .or_else(|| line.procurement_confirmation_line_id.clone())
            .ok_or_else(|| Error::BusinessLogicError("采购变更商品行缺少销售稳定行".to_string()))?;
        let sales_line = sales_lines.get(&stable_id).ok_or_else(|| {
            Error::BusinessLogicError("采购变更商品行在销售当前版本中没有对应稳定行".to_string())
        })?;
        let quantity = line
            .quantity
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("采购变更商品行缺少数量".to_string()))?;
        line.procurement_confirmation_line_id = Some(stable_id.clone());
        line.sales_order_line_id = Some(stable_id);
        line.sales_order_revision_line_id = Some(sales_line.base.id.clone());
        line.sales_order_submission_line_id = None;
        line.allocated_quantity = Some(quantity);
    }
    Ok(enriched)
}

/// 由变更单构造列表/详情视图。
///
/// # 参数
/// * `change` - 变更单
/// * `binding` - 详情时的冻结绑定；列表为空
///
/// # 返回
/// 返回视图。
pub(super) fn change_list_view(
    change: PurchaseChangeOrder,
    binding: Option<ApprovalDefinitionBinding>,
) -> PurchaseChangeOrderView {
    PurchaseChangeOrderView {
        id: change.base.id.clone(),
        purchase_order_id: change.purchase_order_id.to_string(),
        base_revision_id: change.base_revision_id.to_string(),
        reason: change.reason.clone(),
        status: change.stable.status.as_str().to_string(),
        current_submission_id: change.current_submission_id.as_ref().map(ToString::to_string),
        effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
        version: change.base.version,
        created_at: change.base.created_at,
        approval: document_approval_view(binding.as_ref(), None, change.stable.status),
    }
}

/// 内容指纹（Debug 形态 SipHash 十六进制；同二进制内稳定，用于变更目标内容比对）。
pub(super) fn content_fingerprint(lines: &[SavePurchaseOrderLine]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    format!("{:?}", lines).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
