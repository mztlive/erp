//! 供应商跨域只读视图。

use erp_supplier::repository::prelude::*;
use persistence_core::Executor;
pub mod execution_progress;
pub mod fulfillment_detail;
mod fulfillment_display;

pub use fulfillment_detail::SupplierFulfillmentDetailReadService;

pub mod fulfillment_dto;
pub mod offering;
pub mod repository;
pub mod settlement;
pub mod supplier_api;

pub use offering::{MongoOfferingProcurementOwners, SupplierOfferingListView, SupplierOfferingReadService};

/// 按供应商当前主体名称解析身份；空关键词不查询，失败整次返回错误。
async fn keyword_supplier_ids(
    db: &mongodb::Database,
    q: Option<&str>,
    executor: &mut dyn Executor,
) -> crate::Result<Vec<erp_core::ids::SupplierAccountId>> {
    use erp_party::PartyExt;
    use erp_supplier::SupplierExt;
    let Some(q) = application_core::normalized_text(q) else {
        return Ok(Vec::new());
    };
    let parties = db.party().matching_current_party_ids_by_name(&q, executor).await?;
    Ok(db.supplier_accounts().matching_ids_by_parties(&parties, executor).await?)
}
