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
    /// 读取 20 条历史和一个后续探测条目。
    ///
    /// # 参数
    /// * `offering_id` - 已通过范围校验的供给 ID
    /// * `before` - 严格小于此版本号；首页省略
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 至多 21 条、按版本号倒序排列的未删除条款。
    ///
    /// # 错误
    /// 数据库或反序列化错误原样传播。
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

/// 构造稳定供给下的游标条件，始终排除软删除记录。
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
