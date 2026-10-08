//! 把销售 `BusinessType` 适配为工作流单据类型。

use bpm::SubjectRef;
use erp_core::Result;
use erp_sales::entity::sales_order::BusinessType;
use erp_workflow::entity::approval_integration::{
    SalesBusinessKind, document_type_of_sales_business as map_kind,
    subject_ref_for_sales_business as subject_kind,
};
use erp_workflow::entity::document_registry::DocumentType;

/// 把销售业务性质映射为唯一的工作流单据类型。
///
/// # 参数
/// * `business_type` - 销售单业务性质。
///
/// # 返回
/// 返回实物及服务或卡券对应的单据类型。
///
/// # 错误
/// 不返回错误。
pub fn document_type_of_sales_business(business_type: BusinessType) -> DocumentType {
    map_kind(sales_kind(business_type))
}

/// 按销售业务性质和主键构造主体引用。
///
/// # 参数
/// * `business_type` - 销售单业务性质。
/// * `business_object_id` - 销售单主键。
///
/// # 返回
/// 返回对应单据类型的主体引用。
///
/// # 错误
/// 主键为空、仅空白或超长时返回下层构造错误。
pub fn subject_ref_for_sales_business(
    business_type: BusinessType,
    business_object_id: &str,
) -> Result<SubjectRef> {
    subject_kind(sales_kind(business_type), business_object_id)
}

fn sales_kind(business_type: BusinessType) -> SalesBusinessKind {
    match business_type {
        BusinessType::GoodsService => SalesBusinessKind::GoodsService,
        BusinessType::Voucher => SalesBusinessKind::Voucher,
    }
}
