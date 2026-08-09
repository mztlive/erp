//! 域 D05 `file_asset` 仓储：file_asset、document_attachment。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `extensions::FileAssetExt` 关联
//! 常量导入（conventions §4.3）。
//!
//! 筛选/行类型定义在本文件，经 `FileAssetExt` 的关联类型对外暴露。

use entities::file_asset::{
    DocumentAttachment, FileAsset, RetentionClass, SecurityScanStatus, SensitivityClass,
};
use entities::ids::BusinessDocumentId;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::FileAssetExt;
use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `file_asset` 集合名（单一来源：`FileAssetExt` 关联常量）。
const FILE_ASSETS: &str = <mongodb::Database as FileAssetExt>::FILE_ASSETS;
/// `document_attachment` 集合名（单一来源：`FileAssetExt` 关联常量）。
const DOCUMENT_ATTACHMENTS: &str = <mongodb::Database as FileAssetExt>::DOCUMENT_ATTACHMENTS;

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

    /// 按对象存储键精确查找文件资产。
    ///
    /// 查询覆盖 `uk_file_assets_storage_key` 唯一索引；对象键是加密受控存储的
    /// 不可猜测键，本方法用于幂等校验（同一对象键不得重复登记）。
    ///
    /// # 参数
    /// * `storage_object_key` - 对象存储键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除资产；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_storage_key(
        &self,
        storage_object_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<FileAsset>> {
        self.find_one_by_field("storage_object_key", storage_object_key, executor)
            .await
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

/// D05 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `FileAssetExt::file_asset()` 访问。
pub struct FileAssetRepository<'a> {
    db: &'a Database,
}

impl<'a> FileAssetRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 登记文件资产并建立单据附件关联（跨集合多步骤写入）。
    ///
    /// 依次写入 `file_assets` 与 `document_attachments`，保证「资产登记 +
    /// 关联留痕」原子可见（数据模型 §6.1 / §4.5.7）。安全检查、保留期与
    /// 销毁状态不构成关联前置条件。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，关联失败会留下没有关联的孤儿资产；Service
    /// 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `asset` - 待登记的文件资产
    /// * `attachment` - 待建立的附件关联
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn attach_document(
        &self,
        asset: &FileAsset,
        attachment: &DocumentAttachment,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<FileAsset>(FILE_ASSETS), asset, executor).await?;
        mongo_ops::insert_one(
            &self.db.collection::<DocumentAttachment>(DOCUMENT_ATTACHMENTS),
            attachment,
            executor,
        )
        .await?;
        Ok(())
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
    use super::{sort_doc, FileAssetFilter, QueryFilter};
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
}
