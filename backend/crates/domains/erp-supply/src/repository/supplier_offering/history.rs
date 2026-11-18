//! 按供给与版本号查询条款历史，复用已有供给/版本唯一索引。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::SupplierOfferingId;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};

use crate::entity::supplier_offering::SupplierOfferingRevision;
use crate::repository::owned::SupplierOfferingRevisionRepository;

/// 单个供给条款历史的有界游标查询。
#[allow(async_fn_in_trait)]
pub trait OfferingHistoryRepositoryExt {
    /// 在调用方事务中按已授权供给读取未删除条款，按修订号倒序取最多 21 条；历史页用最后一条做续页探测。
    /// 续页只取小于游标的修订，读取或反序列化失败向上传播错误。
    async fn history_page(
        &self,
        offering_id: &SupplierOfferingId,
        before: Option<u32>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOfferingRevision>>;
}

impl OfferingHistoryRepositoryExt for SupplierOfferingRevisionRepository<'_> {
    async fn history_page(
        &self,
        offering_id: &SupplierOfferingId,
        before: Option<u32>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOfferingRevision>> {
        mongo_ops::find_many(
            &self.collection(),
            history_filter(offering_id, before),
            FindOptions::builder().sort(doc! { "revision_no": -1 }).limit(21).build(),
            executor,
        )
        .await
    }
}

// 每页都按同一供给 ID 排除软删除条款；有游标时加严格小于该修订号的条件，避免续页重复返回边界行。
fn history_filter(offering_id: &SupplierOfferingId, before: Option<u32>) -> Document {
    let mut filter =
        doc! { "supplier_offering_id": offering_id.as_ref(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
    if let Some(before) = before {
        filter.insert("revision_no", doc! { "$lt": i64::from(before) });
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_filter_keeps_identity_and_soft_delete_scope_on_every_page() {
        let id = SupplierOfferingId::new("offering-a");
        let first = history_filter(&id, None);
        assert_eq!(first.get_str("supplier_offering_id").unwrap(), "offering-a");
        assert_eq!(first.get_i64("deleted_at").unwrap(), NOT_DELETED_TIMESTAMP_BSON);
        assert!(!first.contains_key("revision_no"));

        let next = history_filter(&id, Some(21));
        assert_eq!(next.get_str("supplier_offering_id").unwrap(), "offering-a");
        assert_eq!(next.get_document("revision_no").unwrap().get_i64("$lt").unwrap(), 21);
    }
}
