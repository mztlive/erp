//! 采购变更的冻结内容与消费方事实映射规则。
use erp_core::ids::PurchaseChangeSubmissionId;
use id_generator::next_id;
use persistence_core::NoTransaction;

use crate::dto::purchase_order::{SavePurchaseOrderLine, SubmitPurchaseChangeRequest};
use crate::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionData, PurchaseOrder,
    PurchaseOrderRevision,
};
use crate::repository::PurchaseOrderExt;
use crate::service::purchase_order::PurchaseOrderService;
use crate::service::purchase_order::line_input::{compute_request_totals, to_line_inputs};
use crate::{Error, Result};

/// 已校验的采购提交金额和待解析付款代码；在外域付款解析前计算。
pub struct ChangeSubmissionHeader {
    /// 含税金额。
    gross: erp_core::money::Amount,
    /// 未税金额。
    net: erp_core::money::Amount,
    /// 税额。
    tax: erp_core::money::Amount,
    /// 按请求覆盖或基准快照回退的付款代码。
    pub payment_term_code: String,
}
/// 保持原行校验、金额计算和付款代码回退先于付款提供方解析。
///
/// # 错误
/// 行或金额非法时返回原采购错误。
pub fn prepare_submission_header(
    base_revision: &PurchaseOrderRevision,
    req: &SubmitPurchaseChangeRequest,
) -> Result<ChangeSubmissionHeader> {
    let inputs = to_line_inputs(&req.lines)?;
    let (gross, net, tax) = compute_request_totals(&inputs)?;
    let payment_term_code = req
        .payment_term_code
        .clone()
        .unwrap_or_else(|| base_revision.payment_term_snapshot.payment_term_code.clone());
    Ok(ChangeSubmissionHeader { gross, net, tax, payment_term_code })
}

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
    pub async fn change_lines_from_base_revision(
        &self,
        revision_id: &erp_core::ids::PurchaseOrderRevisionId,
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        let lines = self.db.purchase_order().list_revision_lines(revision_id, &mut NoTransaction).await?;
        if lines.is_empty() {
            return Err(Error::BusinessLogicError("采购变更基准版本缺少明细".to_string()));
        }
        Ok(lines
            .into_iter()
            .map(|line| {
                let is_item = line.line_type == crate::entity::purchase_order::PurchaseLineType::ItemService;
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
                    gross_amount: if is_item { None } else { Some(line.gross_amount.to_string()) },
                }
            })
            .collect())
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
    /// 构建变更提交（表头取自目标内容，提交动作由调用方冻结审计人）。
    pub async fn build_change_submission(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        base_revision: &PurchaseOrderRevision,
        header: ChangeSubmissionHeader,
        payment_term_snapshot: crate::entity::purchase_order::PaymentTermSnapshot,
    ) -> Result<PurchaseChangeSubmission> {
        let ChangeSubmissionHeader { gross, net, tax, .. } = header;
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
pub fn enrich_change_lines(
    lines: &[SavePurchaseOrderLine],
    sales_lines: &std::collections::HashMap<String, crate::ports::change::CurrentSalesRevisionLineFact>,
) -> Result<Vec<SavePurchaseOrderLine>> {
    let mut enriched = lines.to_vec();
    for line in &mut enriched {
        if line.line_type == crate::entity::purchase_order::PurchaseLineType::LogisticsFee {
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
        line.sales_order_revision_line_id = Some(sales_line.revision_line_id.clone());
        line.sales_order_submission_line_id = None;
        line.allocated_quantity = Some(quantity);
    }
    Ok(enriched)
}
/// 内容指纹（Debug 形态 SipHash 十六进制；同二进制内稳定，用于变更目标内容比对）。
pub fn content_fingerprint(lines: &[SavePurchaseOrderLine]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    format!("{:?}", lines).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::entity::purchase_order::PurchaseLineType;
    use crate::ports::change::CurrentSalesRevisionLineFact;

    fn item() -> SavePurchaseOrderLine {
        SavePurchaseOrderLine {
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some("stable-fallback".into()),
            sku_id: Some("sku-1".into()),
            sku_revision_id: Some("sku-rev-1".into()),
            product_name: Some("商品".into()),
            specification: None,
            quantity: Some("2".into()),
            base_unit_code: Some("piece".into()),
            unit_cost_gross: Some("10".into()),
            input_tax_rate: Some("0".into()),
            expected_delivery_date: None,
            sales_order_line_id: Some("stable-1".into()),
            sales_order_revision_line_id: Some("old-revision-line".into()),
            sales_order_submission_line_id: Some("historical-submit-line".into()),
            allocated_quantity: Some("1".into()),
            gross_amount: None,
        }
    }

    #[test]
    fn current_sales_fact_rebinds_stable_line_and_uses_changed_quantity() {
        let facts = HashMap::from([(
            "stable-1".into(),
            CurrentSalesRevisionLineFact { revision_line_id: "current-line-1".into() },
        )]);
        let result = enrich_change_lines(&[item()], &facts).unwrap();
        let line = &result[0];
        assert_eq!(line.procurement_confirmation_line_id.as_deref(), Some("stable-1"));
        assert_eq!(line.sales_order_line_id.as_deref(), Some("stable-1"));
        assert_eq!(line.sales_order_revision_line_id.as_deref(), Some("current-line-1"));
        assert_eq!(line.sales_order_submission_line_id, None);
        assert_eq!(line.allocated_quantity.as_deref(), Some("2"));
    }

    #[test]
    fn logistics_clears_sales_links_without_requiring_any_sales_fact() {
        let mut line = item();
        line.line_type = PurchaseLineType::LogisticsFee;
        let result = enrich_change_lines(&[line], &HashMap::new()).unwrap();
        assert_eq!(result[0].procurement_confirmation_line_id.as_deref(), Some("stable-fallback"));
        assert_eq!(result[0].sales_order_line_id, None);
        assert_eq!(result[0].sales_order_revision_line_id, None);
        assert_eq!(result[0].sales_order_submission_line_id, None);
        assert_eq!(result[0].allocated_quantity, None);
    }

    #[test]
    fn missing_current_sales_line_precedes_quantity_and_fallback_remains_supported() {
        let mut line = item();
        line.sales_order_line_id = None;
        line.quantity = None;
        let error = enrich_change_lines(&[line.clone()], &HashMap::new()).unwrap_err();
        assert!(
            matches!(error, Error::BusinessLogicError(message) if message == "采购变更商品行在销售当前版本中没有对应稳定行")
        );
        let facts = HashMap::from([(
            "stable-fallback".into(),
            CurrentSalesRevisionLineFact { revision_line_id: "current-fallback".into() },
        )]);
        let error = enrich_change_lines(&[line.clone()], &facts).unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message) if message == "采购变更商品行缺少数量"));
        line.quantity = Some("3".into());
        let result = enrich_change_lines(&[line], &facts).unwrap();
        assert_eq!(result[0].sales_order_line_id.as_deref(), Some("stable-fallback"));
        assert_eq!(result[0].sales_order_revision_line_id.as_deref(), Some("current-fallback"));
        assert_eq!(result[0].allocated_quantity.as_deref(), Some("3"));
    }
}
