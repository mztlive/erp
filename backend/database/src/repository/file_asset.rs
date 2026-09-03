//! 域 D05 `file_asset` 仓储：file_asset、document_attachment。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `extensions::FileAssetExt` 关联
//! 常量导入（conventions §4.3）。
//!
//! 筛选/行类型定义在本文件，经 `FileAssetExt` 的关联类型对外暴露。

use std::collections::HashSet;

use entities::file_asset::{
    DocumentAttachment, FileAsset, RetentionClass, SecurityScanStatus, SensitivityClass,
};
use entities::ids::{BusinessDocumentId, FileAssetId};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 文件资产列表投影行（列表接口只取必要字段，禁止返回整文档）。
///
/// `storage_object_key` 是敏感对象键（§6.1：对象存储地址不得写业务日志），
/// 不进入列表投影，也不进入本行的 `Debug` 输出。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileAssetRow {
    /// 实体主键。
    pub id: String,
    /// 展示文件名。
    pub file_name: String,
    /// 内容类型。
    pub content_type: String,
    /// 字节大小。
    pub byte_size: u64,
    /// 安全检查状态。
    pub security_scan_status: SecurityScanStatus,
    /// 敏感级别。
    pub sensitivity_class: SensitivityClass,
    /// 保留策略。
    pub retention_class: RetentionClass,
    /// 到期时间（秒级时间戳）。
    pub expires_at: Option<u64>,
    /// 创建人。
    pub created_by: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 文件资产列表筛选条件。
#[derive(Debug, Clone)]
pub struct FileAssetFilter {
    /// 文件名（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub file_name: Option<String>,
    /// 安全检查状态；`None` 表示不筛选。
    pub security_scan_status: Option<SecurityScanStatus>,
    /// 保留策略；`None` 表示不筛选。
    pub retention_class: Option<RetentionClass>,
    /// 敏感级别；`None` 表示不筛选。
    pub sensitivity_class: Option<SensitivityClass>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for FileAssetFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "file_name", self.file_name.as_deref());
        if let Some(security_scan_status) = self.security_scan_status {
            filter.insert("security_scan_status", security_scan_status.as_str());
        }
        if let Some(retention_class) = self.retention_class {
            filter.insert("retention_class", retention_class.as_str());
        }
        if let Some(sensitivity_class) = self.sensitivity_class {
            filter.insert("sensitivity_class", sensitivity_class.as_str());
        }
        filter
    }
}

impl Pagination for FileAssetFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, FileAsset> {
    /// 在调用方执行器内有序批量创建文件资产。
    ///
    /// 空集合直接返回且不访问数据库；MongoDB 默认的 ordered 插入保证首个失败
    /// 后不继续写入，完整原子性仍由调用方事务负责。
    ///
    /// # 参数
    /// * `assets` - 按业务命令顺序排列的文件资产
    /// * `executor` - 调用方事务或非事务执行器
    ///
    /// # 错误
    /// 插入失败时返回包含 MongoDB 批量写错误索引的仓储错误。
    pub async fn create_many_ordered(&self, assets: &[FileAsset], executor: &mut dyn Executor) -> Result<()> {
        if assets.is_empty() {
            return Ok(());
        }
        mongo_ops::insert_many(&self.collection(), assets.to_vec(), executor).await?;
        Ok(())
    }

    /// 批量按文件资产 ID 读取活跃文件事实。
    ///
    /// 查询复用通用仓储的未软删除过滤；空 ID 集合直接返回空结果，不访问
    /// MongoDB。返回顺序不承诺与输入一致，由 Service 按业务输入顺序解释缺失。
    ///
    /// # 参数
    /// * `ids` - 文件资产 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未软删除的文件资产事实。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_ids(
        &self,
        ids: &[FileAssetId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FileAsset>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 返回给定文件资产 ID 中尚未登记的缺失 ID。
    ///
    /// 基于 [`Repository::find_by_ids`] 复用未软删除过滤取回已存在事实，
    /// 再与输入做差集；本方法是文件资产存在性判定的唯一属主实现。
    ///
    /// # 参数
    /// * `ids` - 待校验的文件资产 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未命中的 ID，保持输入顺序并去除重复值；输入为空时返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只查询 `file_assets` 集合，不跨聚合访问其他集合。
    pub async fn missing_file_asset_ids(
        &self,
        ids: &[FileAssetId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<FileAssetId>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let found = self.find_by_ids(ids, executor).await?;
        let existing = found
            .into_iter()
            .map(|asset| asset.base.id)
            .collect::<HashSet<_>>();
        let mut missing = Vec::new();
        let mut seen = HashSet::new();
        for id in ids {
            if !existing.contains(id.as_ref()) && seen.insert(id.to_string()) {
                missing.push(id.clone());
            }
        }
        Ok(missing)
    }

    /// 分页检索文件资产列表（投影查询）。
    ///
    /// 只返回 [`FileAssetRow`] 所需的展示与治理字段，不加载整文档；
    /// `file_name` 按字面量忽略大小写模糊匹配（复用 `repository::regex_filter`），
    /// 状态/保留策略/敏感级别精确匹配覆盖 `idx_file_assets_scan_retention`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_file_assets(
        &self,
        filter: &FileAssetFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<FileAssetRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(file_asset_projection())
            .build();
        let collection = self.collection().clone_with_type::<FileAssetRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, DocumentAttachment> {
    /// 按业务单据批量取回附件关联（`idx_document_attachments_document`，无 N+1）。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按创建时间升序排列的附件关联。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_document(
        &self,
        document_id: &BusinessDocumentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentAttachment>> {
        self.find_many_sorted(
            doc! { "document_id": document_id.to_string() },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }
}

/// 构建排序文档（排序字段白名单化，禁止透传任意字段名）。
///
/// 仅允许 `created_at` / `updated_at`；未知字段回落默认 `created_at`。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或白名单外字段时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        _ => "created_at",
    };
    doc! { field: direction }
}

/// 文件资产列表投影字段（不含敏感对象键）。
///
/// # 返回
/// 返回投影条件文档。
fn file_asset_projection() -> Document {
    doc! {
        "id": 1,
        "file_name": 1,
        "content_type": 1,
        "byte_size": 1,
        "security_scan_status": 1,
        "sensitivity_class": 1,
        "retention_class": 1,
        "expires_at": 1,
        "created_by": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, FileAsset, FileAssetFilter, QueryFilter, Repository};
    use crate::NoTransaction;
    use entities::file_asset::{RetentionClass, SecurityScanStatus, SensitivityClass};
    use mongodb::bson::doc;

    #[test]
    fn filter_applies_name_regex_and_class_filters() {
        let filter = FileAssetFilter {
            file_name: Some("导入清单.xlsx".to_string()),
            security_scan_status: Some(SecurityScanStatus::Pending),
            retention_class: Some(RetentionClass::ThirtyDays),
            sensitivity_class: Some(SensitivityClass::Sensitive),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        let name = document.get_document("file_name").unwrap();
        assert_eq!(name.get_str("$regex").unwrap(), r"导入清单\.xlsx");
        assert_eq!(document.get_str("security_scan_status").unwrap(), "pending");
        assert_eq!(document.get_str("retention_class").unwrap(), "thirty_days");
        assert_eq!(document.get_str("sensitivity_class").unwrap(), "sensitive");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("updated_at"), true), doc! { "updated_at": 1 });
        assert_eq!(
            sort_doc(Some("file_name"), false),
            doc! { "created_at": -1 },
            "白名单外字段回落默认排序"
        );
    }

    #[tokio::test]
    async fn find_by_ids_returns_empty_without_touching_database() {
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .unwrap();
        let database = client.database("repository_file_asset_empty_ids");
        let repository = Repository::<FileAsset>::new(&database, "file_assets");

        let assets = repository.find_by_ids(&[], &mut NoTransaction).await.unwrap();

        assert!(assets.is_empty());
    }
}
