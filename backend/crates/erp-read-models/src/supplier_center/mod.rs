//! 供应商跨域只读视图。

pub mod fulfillment_detail;

pub use fulfillment_detail::SupplierFulfillmentDetailReadService;

pub mod fulfillment_access;
pub mod fulfillment_dto;
pub mod offering;
pub mod repository;
pub mod settlement;
pub mod supplier_api;

pub use offering::SupplierOfferingReadService;

/// 按供应商当前主体名称解析身份；空关键词不查询，失败整次返回错误。
async fn keyword_supplier_ids(
    db: &mongodb::Database,
    q: Option<&str>,
) -> crate::Result<Vec<erp_core::ids::SupplierAccountId>> {
    use erp_party::PartyExt;
    use erp_supplier::SupplierExt;
    use persistence_core::NoTransaction;
    let Some(q) = application_core::normalized_text(q) else {
        return Ok(Vec::new());
    };
    let parties = db
        .party()
        .matching_current_party_ids_by_name(&q, &mut NoTransaction)
        .await?;
    Ok(db
        .supplier_accounts()
        .matching_ids_by_parties(&parties, &mut NoTransaction)
        .await?)
}
