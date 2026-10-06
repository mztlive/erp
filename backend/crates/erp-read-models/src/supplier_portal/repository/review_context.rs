//! 新品审核既有供给的精确查询，供应商和订货码始终为并列等值条件。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_supply::SupplierOfferingExt;
use erp_supply::entity::supplier_offering::{OfferingSourceType, OfferingStatus, SupplierOffering};
use erp_supply::portal::{OfferingApplication, PortalSupplyExt};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use super::query::aggregate;
use crate::{Error, Result};

/// 按精确供应商和供给获取待确认修订与停供申请，使用101项探测上限。
///
/// # 参数
/// 数据库、已验证绑定的供应商、已证明归属的供给及调用方执行器。
/// # 返回
/// 返回至多100项冻结申请事实，不混入其他供给的申请。
/// # 错误
/// 超限或持久化错误时拒绝，不截断或猜测结果。
pub(in crate::supplier_portal) async fn pending_applications(
    db: &Database,
    supplier_id: &str,
    offering_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<OfferingApplication>> {
    let rows = aggregate::<OfferingApplication>(
        db.portal_applications().collection().clone_with_type::<Document>(),
        vec![
            doc! {"$match":pending_filter(supplier_id,offering_id)},
            doc! {"$sort":{"created_at":-1,"id":1}},
            doc! {"$limit":101},
        ],
        executor,
    )
    .await?;
    if rows.len() > 100 {
        return Err(Error::ValidationError("该供给待确认申请超过100项，请先处理已有申请".into()));
    }
    Ok(rows)
}

fn pending_filter(supplier_id: &str, offering_id: &str) -> Document {
    doc! {"supplier_id":supplier_id,"snapshot.offering_id":offering_id,
        "status":"SUBMITTED",
    "snapshot.kind":{"$in":["TERMS_CHANGE","STOP_SUPPLY"]},
    "deleted_at":NOT_DELETED_TIMESTAMP_BSON}
}

/// 查询精确本供应商及本行订货码对应的可维护供给。
///
/// # 参数
/// 数据库、已由申请解析的供应商、原订货码及调用方执行器。
/// # 返回
/// 返回精确候选；上层仍须逐个执行供给对象范围读取授权。
/// # 错误
/// 数据库或领域仓储错误保持失败关闭。
pub(in crate::supplier_portal) async fn existing_offerings(
    db: &Database,
    supplier_id: &str,
    ordering_code: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierOffering>> {
    Ok(db.supplier_offerings().find_many(existing_filter(supplier_id, ordering_code), executor).await?)
}

fn existing_filter(supplier_id: &str, ordering_code: &str) -> Document {
    doc! {
        "supplier_id": supplier_id,
        "supplier_sku_code": ordering_code,
        "source_type": {"$ne": OfferingSourceType::Api.as_str()},
        "status": {"$ne": OfferingStatus::Stopped.as_str()},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_filter_requires_exact_supplier_and_ordering_code() {
        let filter = existing_filter("supplier-a", "SKU.*001");
        assert_eq!(filter.get_str("supplier_id").unwrap(), "supplier-a");
        assert_eq!(filter.get_str("supplier_sku_code").unwrap(), "SKU.*001");
        assert!(!filter.contains_key("$or"));
        assert_eq!(filter.get_document("source_type").unwrap().get_str("$ne").unwrap(), "API");
        assert_eq!(filter.get_document("status").unwrap().get_str("$ne").unwrap(), "STOPPED");
    }

    #[test]
    fn pending_filter_requires_exact_source_and_submitted_change_kind() {
        let filter = pending_filter("supplier-a", "offering-a");
        assert_eq!(filter.get_str("supplier_id").unwrap(), "supplier-a");
        assert_eq!(filter.get_str("snapshot.offering_id").unwrap(), "offering-a");
        assert_eq!(filter.get_str("status").unwrap(), "SUBMITTED");
        assert_eq!(filter.get_document("snapshot.kind").unwrap().get_array("$in").unwrap().len(), 2);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), NOT_DELETED_TIMESTAMP_BSON);
        assert!(!filter.contains_key("$or"));
    }
}
