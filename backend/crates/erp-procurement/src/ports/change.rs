//! 采购变更消费的来源销售最小事实；提供方在流程中显式映射。
use erp_core::ids::SkuId;

/// 当前销售版本行的稳定引用；键由调用方保持原稳定销售行 ID。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSalesRevisionLineFact {
    /// 当前销售正式版本行 ID，原缺稳定行规则由采购判断。
    pub revision_line_id: String,
    /// 精确当前正式销售商品行的 SKU 身份。
    pub sku_id: SkuId,
}
