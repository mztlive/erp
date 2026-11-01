//! 支撑领域读取业务单据注册事实的消费方端口。

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 支撑领域消费的工作流 `DocumentType` 序列化代码。
///
/// 支撑领域不依赖 `erp-workflow`。组合适配器把这些代码映射到工作流目录；
/// 本列表是二十个已冻结 snake_case 变体的消费方快照。条目按二分查找排序，
/// 新增代码时必须保持有序。
pub const BUSINESS_DOCUMENT_TYPE_CODES: &[&str] = &[
    "customer_acceptance",
    "customer_receipt",
    "customer_refund",
    "delivery",
    "electronic_delivery",
    "invoice",
    "payment_reversal",
    "purchase_change_order",
    "purchase_order",
    "purchase_receipt",
    "purchase_return_order",
    "receipt_reversal",
    "sales_change_order",
    "sales_order",
    "sales_return_case",
    "service_fulfillment",
    "stock_adjustment",
    "supplier_payment",
    "supplier_refund",
    "voucher_sales_order",
];

/// 判断 `object_type` 是否为已冻结的业务单据类型码。
///
/// 匹配为精确且失败关闭：空白、别名和大小写折叠都会被拒绝，
/// 使批量任务目标校验保持原有 `DocumentType` 合同。
///
/// # 参数
/// * `object_type` - 待判断的类型码
///
/// # 返回
/// 与 [`BUSINESS_DOCUMENT_TYPE_CODES`] 中某项完全一致时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub fn is_business_document_type(object_type: &str) -> bool {
    BUSINESS_DOCUMENT_TYPE_CODES.binary_search(&object_type).is_ok()
}

/// 支撑领域读取已注册业务单据 ID、且不依赖工作流类型的端口。
#[async_trait]
pub trait BusinessDocumentPort: Send + Sync {
    /// 确认单个业务单据 ID 已注册。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 已注册时无返回值。
    ///
    /// # 错误
    /// 单据未注册或端口无法确认时返回错误。
    async fn ensure_registered(&self, document_id: &str, executor: &mut dyn Executor) -> Result<()>;

    /// 从给定集合中加载已注册的单据 ID。
    ///
    /// # 参数
    /// * `ids` - 待核对的单据 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回其中已注册的 ID。
    ///
    /// # 错误
    /// 查询失败时返回错误。
    async fn find_registered_ids(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<String>>;
}

/// 组合根尚未注入适配器时使用的失败关闭单据端口。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedBusinessDocumentPort;

#[async_trait]
impl BusinessDocumentPort for FailClosedBusinessDocumentPort {
    /// 拒绝确认未接线的业务单据端口。
    ///
    /// # 参数
    /// * `_document_id` - 业务单据 ID；本实现不查询
    /// * `_executor` - 调用方执行器；本实现不访问
    ///
    /// # 返回
    /// 不返回成功。
    ///
    /// # 错误
    /// 始终返回 `Internal`（业务单据端口未接线）。
    async fn ensure_registered(&self, _document_id: &str, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("业务单据端口未接线".to_string()))
    }

    /// 拒绝读取未接线的业务单据端口。
    ///
    /// # 参数
    /// * `_ids` - 待核对的单据 ID；本实现不查询
    /// * `_executor` - 调用方执行器；本实现不访问
    ///
    /// # 返回
    /// 不返回成功。
    ///
    /// # 错误
    /// 始终返回 `Internal`（业务单据端口未接线）。
    async fn find_registered_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(Error::Internal("业务单据端口未接线".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{BUSINESS_DOCUMENT_TYPE_CODES, is_business_document_type};

    #[test]
    fn business_document_type_matching_is_exact_and_fail_closed() {
        assert!(is_business_document_type("sales_order"));
        assert!(is_business_document_type("payment_reversal"));
        assert!(!is_business_document_type(" Sales_order "));
        assert!(!is_business_document_type("SALES_ORDER"));
        assert!(!is_business_document_type("unknown"));
        assert_eq!(BUSINESS_DOCUMENT_TYPE_CODES.len(), 20);
        let mut sorted = BUSINESS_DOCUMENT_TYPE_CODES.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, BUSINESS_DOCUMENT_TYPE_CODES, "类型码须保持有序以支持二分查找");
    }
}
