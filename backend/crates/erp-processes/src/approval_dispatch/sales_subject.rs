//! Sales `BusinessType` adapters onto workflow document-type mapping.

use bpm::SubjectRef;
use erp_core::Result;
use erp_sales::entity::sales_order::BusinessType;
use erp_workflow::entity::approval_integration::{
    document_type_of_sales_business as map_kind, subject_ref_for_sales_business as subject_kind,
    SalesBusinessKind,
};
use erp_workflow::entity::document_registry::DocumentType;

/// Map sales business nature onto the unique workflow document type.
pub fn document_type_of_sales_business(business_type: BusinessType) -> DocumentType {
    map_kind(sales_kind(business_type))
}

/// Build a subject ref from sales business nature.
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
