//! 费用采购单创建与费用行确认发生命令（§7.5.1）。

use erp_core::common::time::BusinessDate;
use erp_core::ids::{FileAssetId, SalesOrderLineId, SupplierAccountId};
use erp_core::money::{Amount, Rate};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::purchase_order::{InquiryExpenseSource, PurchaseExpenseCategory, payload_fingerprint};
use crate::{Error, Result};

/// 费用采购单创建命令的稳定动作。
pub const CREATE_EXPENSE_ACTION: &str = "purchase_order.create_expense";
/// 费用行确认发生命令的稳定动作。
pub const CONFIRM_EXPENSE_ACTION: &str = "purchase_order.confirm_expense";
/// 一张费用采购单最多费用行数。
pub const EXPENSE_LINE_LIMIT: usize = 50;

/// 一条待创建的费用行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateExpenseLineRequest {
    /// 费用类别。
    pub category: PurchaseExpenseCategory,
    /// 费用说明。
    pub description: String,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 归属销售明细；空表示整张销售单。
    #[serde(default)]
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 可选询价费用来源；服务端在建单事务内重验，不信任客户端声明。
    #[serde(default)]
    pub inquiry_source: Option<InquiryExpenseSource>,
}

/// 从销售单追加费用采购单的命令；创建后立即提交审批。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateExpensePurchaseRequest {
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 付款条件代码。
    #[validate(length(min = 1, max = 64, message = "请选择付款条件"))]
    pub payment_term_code: String,
    /// 费用行。
    #[validate(length(min = 1, max = 50, message = "费用行须为 1–50 行"))]
    pub lines: Vec<CreateExpenseLineRequest>,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl CreateExpensePurchaseRequest {
    /// 计算命令载荷指纹；幂等键不参与。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    ///
    /// # 返回
    /// 返回稳定指纹。
    ///
    /// # 错误
    /// 载荷序列化失败时返回错误。
    pub fn request_fingerprint(&self, sales_order_id: &str) -> Result<String> {
        let payload = (&self.supplier_id, self.payment_term_code.trim(), &self.lines);
        payload_fingerprint(CREATE_EXPENSE_ACTION, sales_order_id, &payload).map_err(Error::from)
    }
}

/// 费用行确认发生命令；凭证随命令 multipart 提交。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ConfirmExpenseOccurrenceRequest {
    /// 发生日期。
    pub occurred_on: BusinessDate,
    /// 凭证文件；本次上传时为 `pending-file:` 临时引用。
    pub evidence_attachment_id: FileAssetId,
}

/// 费用行确认发生结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfirmExpenseOccurrenceResult {
    /// 费用行身份。
    pub expense_line_id: String,
    /// 实际发生成本事实。
    pub cost_entry_id: String,
    /// 是否为已确认事实的重放。
    pub replayed: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body() -> serde_json::Value {
        json!({
            "supplier_id": "sup-1", "payment_term_code": "NET-30",
            "lines": [{"category": "logistics", "description": "第三方物流", "gross_amount": "106.00", "input_tax_rate": "0.06"}],
            "idempotency_key": "k-1"
        })
    }

    #[test]
    fn create_request_validates_lines_and_fingerprint_ignores_key() {
        let request: CreateExpensePurchaseRequest = serde_json::from_value(body()).unwrap();
        assert!(request.validate().is_ok());
        let mut other_key = request.clone();
        other_key.idempotency_key = "k-2".into();
        assert_eq!(
            request.request_fingerprint("so-1").unwrap(),
            other_key.request_fingerprint("so-1").unwrap()
        );
        assert_ne!(
            request.request_fingerprint("so-1").unwrap(),
            request.request_fingerprint("so-2").unwrap()
        );
        let mut empty = request.clone();
        empty.lines.clear();
        assert!(empty.validate().is_err());
        let mut unknown = body();
        unknown["lines"][0]["sku_id"] = json!("sku-1");
        assert!(serde_json::from_value::<CreateExpensePurchaseRequest>(unknown).is_err());
    }
}
