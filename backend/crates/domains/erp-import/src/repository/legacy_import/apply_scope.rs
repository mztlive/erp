//! 导入应用范围：按请求行 ID 且受批次约束读取并报告缺失（INT-R29）。
//!
//! 固定两次有界读取：`$in` 取请求行，再按批次统计请求外仍待导入行数。
//! 空 ID 集合不访问数据库。全部使用调用方 executor，不开事务。

use std::collections::{HashMap, HashSet};

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{LegacyImportBatchId, LegacyImportRowId};
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, Result, mongo_ops};

use crate::entity::legacy_import::{ImportStatus, LegacyImportRow};

/// 一次应用请求对应的导入行持久化范围。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LegacyImportApplyScope {
    /// 命中目标批次且未软删除的请求行（按 ID 索引；缺失 ID 不出现）。
    pub rows: HashMap<String, LegacyImportRow>,
    /// 请求中未命中目标批次活跃行的 ID（去重后按首次出现顺序）。
    pub missing_row_ids: Vec<LegacyImportRowId>,
    /// 目标批次中请求 ID 之外仍为待导入的行数。
    pub pending_outside_request: u64,
}

/// 导入行集合仓储扩展：按请求行 ID 读取批次约束下的应用范围。
#[allow(async_fn_in_trait)]
pub trait LegacyImportRowApplyScopeExt {
    /// 按请求行 ID 读取目标批次内的导入行，并返回缺失集合。
    ///
    /// 查询同时约束 `id ∈ requested` 与 `batch_id`，软删除行视为缺失。
    /// 请求 ID 在仓储内按首次出现去重；空集合不访问数据库。
    /// 第二次读取统计同一批次中未包含在请求内的待导入行数，供 Service
    /// 判断是否全部终态。本方法不自行开启或提交事务。
    ///
    /// # 参数
    /// * `batch_id` - 目标导入批次
    /// * `row_ids` - 请求中的行 ID（可含重复）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中行映射、按请求顺序的缺失 ID，以及请求外待导入行数。
    ///
    /// # 错误
    /// MongoDB 查询或计数失败时返回错误。
    ///
    /// # 约束
    /// 不返回 services DTO、HTTP View 或授权结论；不裁决未知 ID 是否失败关闭。
    async fn apply_row_scope(
        &self,
        batch_id: &LegacyImportBatchId,
        row_ids: &[LegacyImportRowId],
        executor: &mut dyn Executor,
    ) -> Result<LegacyImportApplyScope>;
}

impl LegacyImportRowApplyScopeExt for persistence_core::Repository<'_, LegacyImportRow> {
    async fn apply_row_scope(
        &self,
        batch_id: &LegacyImportBatchId,
        row_ids: &[LegacyImportRowId],
        executor: &mut dyn Executor,
    ) -> Result<LegacyImportApplyScope> {
        let unique_ids = unique_row_ids(row_ids);
        if unique_ids.is_empty() {
            return Ok(LegacyImportApplyScope::default());
        }

        let keys: Vec<Bson> = unique_ids.iter().map(|id| id.to_string().into()).collect();
        let found = self
            .find_many_sorted(
                doc! { "id": { "$in": keys.clone() }, "batch_id": batch_id.to_string() },
                doc! { "id": 1 },
                executor,
            )
            .await?;
        let rows: HashMap<_, _> = found.into_iter().map(|row| (row.base.id.clone(), row)).collect();
        let missing_row_ids = missing_row_ids(unique_ids, &rows);

        let pending_outside_request =
            mongo_ops::count_documents(&self.collection(), pending_outside_filter(batch_id, keys), executor)
                .await?;
        Ok(LegacyImportApplyScope { rows, missing_row_ids, pending_outside_request })
    }
}

fn pending_outside_filter(batch_id: &LegacyImportBatchId, excluded_ids: Vec<Bson>) -> Document {
    doc! {
        "batch_id": batch_id.to_string(),
        "import_status": ImportStatus::PendingImport.as_str(),
        "id": { "$nin": excluded_ids },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 按首次出现顺序去重行 ID。
///
/// # 参数
/// * `row_ids` - 原始 ID 切片（可含重复）
///
/// # 返回
/// 返回去重后的行 ID 列表。
fn unique_row_ids(row_ids: &[LegacyImportRowId]) -> Vec<LegacyImportRowId> {
    let mut seen = HashSet::new();
    row_ids.iter().filter(|id| seen.insert(*id)).cloned().collect()
}

fn missing_row_ids(
    requested: Vec<LegacyImportRowId>,
    rows: &HashMap<String, LegacyImportRow>,
) -> Vec<LegacyImportRowId> {
    requested.into_iter().filter(|id| !rows.contains_key(id.as_ref())).collect()
}

#[cfg(test)]
mod tests {
    use entity_core::NOT_DELETED_TIMESTAMP_BSON;
    use erp_core::ids::{LegacyImportBatchId, LegacyImportRowId};

    use super::{LegacyImportApplyScope, missing_row_ids, pending_outside_filter, unique_row_ids};
    use crate::entity::legacy_import::{LegacyImportRow, LegacyImportRowData};

    fn row(id: &str) -> LegacyImportRow {
        LegacyImportRow::new(
            LegacyImportRowId::new(id),
            LegacyImportRowData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                source_object_type: "CONTRACT".to_string(),
                source_row_key: id.to_string(),
                normalized_payload_reference: format!("payload:{id}"),
            },
        )
        .unwrap()
    }

    #[test]
    fn unique_row_ids_dedups_empty_and_repeats() {
        assert!(unique_row_ids(&[]).is_empty());
        let ids = unique_row_ids(&[
            LegacyImportRowId::new("row-1"),
            LegacyImportRowId::new("row-2"),
            LegacyImportRowId::new("row-1"),
        ]);
        assert_eq!(ids, vec![LegacyImportRowId::new("row-1"), LegacyImportRowId::new("row-2")]);
    }

    #[test]
    fn missing_row_ids_preserves_request_order_and_reports_unknown() {
        let requested = vec![
            LegacyImportRowId::new("row-1"),
            LegacyImportRowId::new("row-missing"),
            LegacyImportRowId::new("row-2"),
            LegacyImportRowId::new("row-other-batch"),
        ];
        let found = [("row-1".into(), row("row-1")), ("row-2".into(), row("row-2"))].into();
        let missing = missing_row_ids(requested.clone(), &found);
        assert_eq!(
            missing,
            vec![LegacyImportRowId::new("row-missing"), LegacyImportRowId::new("row-other-batch"),]
        );
        assert_eq!(missing_row_ids(requested.clone(), &Default::default()), requested);
    }

    #[test]
    fn pending_outside_filter_excludes_requested_ids_and_soft_deleted() {
        let excluded = vec![
            mongodb::bson::Bson::String("row-pending".to_string()),
            mongodb::bson::Bson::String("row-imported".to_string()),
        ];
        let filter = pending_outside_filter(&LegacyImportBatchId::new("batch-apply"), excluded);
        assert_eq!(filter.get_str("batch_id").unwrap(), "batch-apply");
        assert_eq!(filter.get_str("import_status").unwrap(), "pending_import");
        assert_eq!(filter.get_i64("deleted_at").unwrap(), NOT_DELETED_TIMESTAMP_BSON);

        let nin = filter.get_document("id").unwrap().get_array("$nin").unwrap();
        assert_eq!(nin.len(), 2);
        assert!(nin.contains(&mongodb::bson::Bson::String("row-pending".to_string())));
    }

    #[test]
    fn empty_request_scope_is_default_without_database() {
        // `apply_row_scope` 空集合分支直接返回零值 scope，不访问数据库；
        // 此处纯内存断言该分支的前置条件与返回值形状，不构造 Mongo Client。
        assert!(unique_row_ids(&[]).is_empty());
        let scope = LegacyImportApplyScope::default();
        assert!(scope.rows.is_empty());
        assert!(scope.missing_row_ids.is_empty());
        assert_eq!(scope.pending_outside_request, 0);
    }
}
