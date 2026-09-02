//! 导入应用范围：按请求行 ID 且受批次约束读取并报告缺失（INT-R29）。
//!
//! 固定两次有界读取：`$in` 取请求行，再按批次统计请求外仍待导入行数。
//! 空 ID 集合不访问数据库。全部使用调用方 executor，不开事务。

use std::collections::{HashMap, HashSet};

use entities::ids::{LegacyImportBatchId, LegacyImportRowId};
use entities::legacy_import::{ImportStatus, LegacyImportRow};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;

use super::super::Repository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 一次应用请求对应的导入行持久化范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImportApplyScope {
    /// 命中目标批次且未软删除的请求行（按 ID 索引；缺失 ID 不出现）。
    pub rows: HashMap<String, LegacyImportRow>,
    /// 请求中未命中目标批次活跃行的 ID（去重后按首次出现顺序）。
    pub missing_row_ids: Vec<LegacyImportRowId>,
    /// 目标批次中请求 ID 之外仍为待导入的行数。
    pub pending_outside_request: u64,
}

impl<'a> Repository<'a, LegacyImportRow> {
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
    pub async fn apply_row_scope(
        &self,
        batch_id: &LegacyImportBatchId,
        row_ids: &[LegacyImportRowId],
        executor: &mut dyn Executor,
    ) -> Result<LegacyImportApplyScope> {
        let unique_ids = unique_row_ids(row_ids);
        if unique_ids.is_empty() {
            return Ok(LegacyImportApplyScope {
                rows: HashMap::new(),
                missing_row_ids: Vec::new(),
                pending_outside_request: 0,
            });
        }
        let found = self.load_apply_rows(batch_id, &unique_ids, executor).await?;
        let missing_row_ids = missing_row_ids(&unique_ids, &found);
        let pending_outside_request = self
            .count_pending_outside_request(batch_id, &unique_ids, executor)
            .await?;
        Ok(LegacyImportApplyScope {
            rows: index_rows(found),
            missing_row_ids,
            pending_outside_request,
        })
    }

    /// 按 ID 集合与批次约束装载未删除导入行。
    ///
    /// # 参数
    /// * `batch_id` - 目标批次
    /// * `row_ids` - 已去重的行 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回稳定按 `id` 升序排列的命中行。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_apply_rows(
        &self,
        batch_id: &LegacyImportBatchId,
        row_ids: &[LegacyImportRowId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportRow>> {
        let keys: Vec<mongodb::bson::Bson> = row_ids.iter().map(|id| id.to_string().into()).collect();
        self.find_many_sorted(
            doc! {
                "id": { "$in": keys },
                "batch_id": batch_id.to_string(),
            },
            doc! { "id": 1 },
            executor,
        )
        .await
    }

    /// 统计目标批次中未包含在请求内的待导入行。
    ///
    /// # 参数
    /// * `batch_id` - 目标批次
    /// * `row_ids` - 已去重的请求行 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回请求外仍为 `pending_import` 且未软删除的行数。
    ///
    /// # 错误
    /// MongoDB 计数失败时返回错误。
    async fn count_pending_outside_request(
        &self,
        batch_id: &LegacyImportBatchId,
        row_ids: &[LegacyImportRowId],
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        let excluded: Vec<mongodb::bson::Bson> = row_ids.iter().map(|id| id.to_string().into()).collect();
        mongo_ops::count_documents(
            &self.collection(),
            pending_outside_filter(batch_id, excluded),
            executor,
        )
        .await
    }
}

/// 构造请求外待导入行的精确计数条件。
///
/// # 参数
/// * `batch_id` - 目标批次
/// * `excluded_ids` - 已去重请求行 ID 的 BSON 列表
///
/// # 返回
/// 返回含批次、待导入状态、请求 ID `$nin` 与未软删除过滤的文档。
fn pending_outside_filter(
    batch_id: &LegacyImportBatchId,
    excluded_ids: Vec<mongodb::bson::Bson>,
) -> mongodb::bson::Document {
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
    let mut unique = Vec::new();
    for id in row_ids {
        if seen.insert(id.to_string()) {
            unique.push(id.clone());
        }
    }
    unique
}

/// 按请求顺序报告未命中的行 ID。
///
/// # 参数
/// * `requested` - 已去重的请求 ID
/// * `found` - 批次约束下命中的行
///
/// # 返回
/// 返回缺失 ID 列表；空命中时请求全部视为缺失。
fn missing_row_ids(requested: &[LegacyImportRowId], found: &[LegacyImportRow]) -> Vec<LegacyImportRowId> {
    let found_ids: HashSet<&str> = found.iter().map(|row| row.base.id.as_str()).collect();
    requested
        .iter()
        .filter(|id| !found_ids.contains(id.as_ref()))
        .cloned()
        .collect()
}

/// 将命中行按 ID 建索引。
///
/// # 参数
/// * `rows` - 命中的导入行
///
/// # 返回
/// 返回以实体主键为键的映射。
fn index_rows(rows: Vec<LegacyImportRow>) -> HashMap<String, LegacyImportRow> {
    rows.into_iter().map(|row| (row.base.id.clone(), row)).collect()
}

#[cfg(test)]
mod tests {
    use super::{missing_row_ids, pending_outside_filter, unique_row_ids};
    use entities::ids::{LegacyImportBatchId, LegacyImportRowId};
    use entities::legacy_import::{LegacyImportRow, LegacyImportRowData};
    use entity_core::NOT_DELETED_TIMESTAMP_BSON;

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
        assert_eq!(
            ids,
            vec![LegacyImportRowId::new("row-1"), LegacyImportRowId::new("row-2")]
        );
    }

    #[test]
    fn missing_row_ids_preserves_request_order_and_reports_unknown() {
        let requested = vec![
            LegacyImportRowId::new("row-1"),
            LegacyImportRowId::new("row-missing"),
            LegacyImportRowId::new("row-2"),
            LegacyImportRowId::new("row-other-batch"),
        ];
        let found = vec![row("row-1"), row("row-2")];
        let missing = missing_row_ids(&requested, &found);
        assert_eq!(
            missing,
            vec![
                LegacyImportRowId::new("row-missing"),
                LegacyImportRowId::new("row-other-batch"),
            ]
        );
        assert!(missing_row_ids(&requested, &[]).len() == 4);
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

    #[tokio::test]
    async fn apply_row_scope_empty_ids_skip_database() {
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .unwrap();
        let database = client.database("legacy_import_apply_empty");
        let repository = crate::Repository::<LegacyImportRow>::new(&database, "legacy_import_rows");
        let scope = repository
            .apply_row_scope(
                &LegacyImportBatchId::new("batch-1"),
                &[],
                &mut crate::NoTransaction,
            )
            .await
            .unwrap();
        assert!(scope.rows.is_empty());
        assert!(scope.missing_row_ids.is_empty());
        assert_eq!(scope.pending_outside_request, 0);
    }

    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn apply_row_scope_reports_missing_soft_deleted_and_pending_outside() {
        use crate::repository::extensions::LegacyImportExt;
        use crate::{ensure_indexes, NoTransaction};
        use test_support::{require_mongo, TestDb};

        require_mongo!(async {
            let fixture = TestDb::new("legacy_import_apply_scope")
                .await
                .expect("测试数据库创建失败");
            ensure_indexes(fixture.db()).await.expect("索引创建失败");
            let batch_id = LegacyImportBatchId::new("batch-apply");
            let pending_in_request = row_in_batch("row-pending", "batch-apply");
            let already_imported = {
                let mut row = row_in_batch("row-imported", "batch-apply");
                row.import_status = entities::legacy_import::ImportStatus::Imported;
                row
            };
            let pending_outside = row_in_batch("row-outside", "batch-apply");
            let other_batch = row_in_batch("row-other-batch", "batch-other");
            let mut soft_deleted = row_in_batch("row-deleted", "batch-apply");
            let mut soft_deleted_outside = row_in_batch("row-deleted-outside", "batch-apply");
            for row in [
                &pending_in_request,
                &already_imported,
                &pending_outside,
                &other_batch,
                &soft_deleted,
                &soft_deleted_outside,
            ] {
                fixture
                    .db()
                    .legacy_import_rows()
                    .create(row, &mut NoTransaction)
                    .await
                    .expect("导入行写入失败");
            }
            fixture
                .db()
                .legacy_import_rows()
                .soft_delete(&mut soft_deleted, &mut NoTransaction)
                .await
                .expect("软删除失败");
            fixture
                .db()
                .legacy_import_rows()
                .soft_delete(&mut soft_deleted_outside, &mut NoTransaction)
                .await
                .expect("请求外软删除失败");

            let requested = vec![
                LegacyImportRowId::new("row-pending"),
                LegacyImportRowId::new("row-imported"),
                LegacyImportRowId::new("row-pending"),
                LegacyImportRowId::new("row-unknown"),
                LegacyImportRowId::new("row-other-batch"),
                LegacyImportRowId::new("row-deleted"),
            ];
            let scope = fixture
                .db()
                .legacy_import_rows()
                .apply_row_scope(&batch_id, &requested, &mut NoTransaction)
                .await
                .expect("应用范围读取失败");

            assert_eq!(scope.rows.len(), 2);
            assert!(scope.rows.contains_key("row-pending"));
            assert!(scope.rows.contains_key("row-imported"));
            assert_eq!(
                scope.missing_row_ids,
                vec![
                    LegacyImportRowId::new("row-unknown"),
                    LegacyImportRowId::new("row-other-batch"),
                    LegacyImportRowId::new("row-deleted"),
                ]
            );
            assert_eq!(scope.pending_outside_request, 1);
        });
    }

    fn row_in_batch(id: &str, batch_id: &str) -> LegacyImportRow {
        LegacyImportRow::new(
            LegacyImportRowId::new(id),
            LegacyImportRowData {
                batch_id: LegacyImportBatchId::new(batch_id),
                source_object_type: "CONTRACT".to_string(),
                source_row_key: id.to_string(),
                normalized_payload_reference: format!("payload:{id}"),
            },
        )
        .unwrap()
    }
}
