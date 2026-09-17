//! 库存调整申请人读取条件；由组合层对审批快照仓储执行。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use persistence_core::{Pagination, QueryFilter};

/// 审批快照上的库存调整申请人条件；不解释仓库授权。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdjustmentSnapshotReadFilter {
    /// 调整单主键；`None` 不按对象收窄，空集合表示无命中。
    pub business_object_ids: Option<Vec<String>>,
    /// 快照 `submitted_by`；`None` 不按申请人收窄，空集合表示无命中。
    pub submitted_by_ids: Option<Vec<String>>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
}

impl Default for AdjustmentSnapshotReadFilter {
    fn default() -> Self {
        Self { business_object_ids: None, submitted_by_ids: None, page: 1, page_size: 100 }
    }
}

impl QueryFilter for AdjustmentSnapshotReadFilter {
    fn to_doc(&self) -> Document {
        let mut filter = doc! {
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "document_type": "stock_adjustment",
        };
        insert_ids(&mut filter, "business_object_id", self.business_object_ids.as_ref());
        insert_ids(&mut filter, "payload.submitted_by", self.submitted_by_ids.as_ref());
        filter
    }
}

impl Pagination for AdjustmentSnapshotReadFilter {
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

fn insert_ids(filter: &mut Document, field: &str, ids: Option<&Vec<String>>) {
    if let Some(ids) = ids {
        filter.insert(field, doc! { "$in": ids.clone() });
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{Bson, doc};
    use persistence_core::QueryFilter;

    use super::AdjustmentSnapshotReadFilter;

    #[test]
    fn snapshot_filter_keeps_stock_adjustment_and_applicant_ids() {
        let filter = AdjustmentSnapshotReadFilter {
            business_object_ids: Some(vec!["adj-1".into()]),
            submitted_by_ids: Some(vec!["applicant-1".into()]),
            ..Default::default()
        };
        let document = filter.to_doc();
        assert_eq!(document.get_str("document_type").unwrap(), "stock_adjustment");
        assert_eq!(document.get_document("business_object_id").unwrap(), &doc! { "$in": ["adj-1"] });
        assert_eq!(document.get_document("payload.submitted_by").unwrap(), &doc! { "$in": ["applicant-1"] });
        let empty = AdjustmentSnapshotReadFilter { submitted_by_ids: Some(Vec::new()), ..Default::default() };
        assert_eq!(
            empty.to_doc().get_document("payload.submitted_by").unwrap().get_array("$in").unwrap(),
            &Vec::<Bson>::new()
        );
    }
}
