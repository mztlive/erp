//! 域 D01 `source_registry` 仓储：source_system、external_identity_map、external_identity_target。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `indexes::source_registry` 导入。
//!
//! 筛选/行类型定义在本文件，经 `SourceRegistryExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::source_registry::{
    ExternalIdKey, ExternalIdentityMap, ExternalIdentityTarget, ExternalObjectType, MappingStatus,
    SourceSystem, SourceSystemId, SourceSystemStatus, SourceSystemType,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::SourceRegistryExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `external_identity_map` 集合名（单一来源：`SourceRegistryExt` 关联常量）。
const EXTERNAL_IDENTITY_MAPS: &str = <mongodb::Database as SourceRegistryExt>::EXTERNAL_IDENTITY_MAPS;
/// `external_identity_target` 集合名（单一来源：`SourceRegistryExt` 关联常量）。
const EXTERNAL_IDENTITY_TARGETS: &str = <mongodb::Database as SourceRegistryExt>::EXTERNAL_IDENTITY_TARGETS;

/// 来源系统列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSystemRow {
    /// 实体主键。
    pub id: String,
    /// 稳定代码。
    pub code: String,
    /// 显示名称。
    pub name: String,
    /// 系统类型。
    pub system_type: SourceSystemType,
    /// 启停状态。
    pub status: SourceSystemStatus,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 来源系统列表筛选条件。
#[derive(Debug, Clone)]
pub struct SourceSystemFilter {
    /// 代码精确匹配；`None` 表示不筛选。
    pub code: Option<String>,
    /// 系统类型；`None` 表示不筛选。
    pub system_type: Option<SourceSystemType>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<SourceSystemStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SourceSystemFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.code {
            filter.insert("code", code);
        }
        if let Some(system_type) = self.system_type {
            filter.insert("system_type", system_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SourceSystemFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SourceSystem> {
    /// 分页检索来源系统列表（投影查询）。
    ///
    /// 只返回 [`SourceSystemRow`] 所需的列表字段，不加载整文档；
    /// 排序字段由 Service 层白名单校验后传入（api-contract §4）。
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
    pub async fn search_source_systems(
        &self,
        filter: &SourceSystemFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SourceSystemRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(source_system_projection())
            .build();
        let collection = self.collection().clone_with_type::<SourceSystemRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 外部身份映射列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalIdentityMapRow {
    /// 实体主键。
    pub id: String,
    /// 来源系统 ID。
    pub source_system_id: String,
    /// 外部对象类型。
    pub object_type: ExternalObjectType,
    /// 来源原值。
    pub external_id: String,
    /// 映射状态。
    pub mapping_status: MappingStatus,
    /// 映射时间（秒级时间戳）。
    pub mapped_at: Option<u64>,
    /// 映射责任人。
    pub mapped_by: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 外部身份映射列表筛选条件。
#[derive(Debug, Clone)]
pub struct ExternalIdentityMapFilter {
    /// 来源系统 ID；`None` 表示不筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 映射状态；`None` 表示不筛选。
    pub mapping_status: Option<MappingStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ExternalIdentityMapFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(source_system_id) = &self.source_system_id {
            filter.insert("source_system_id", source_system_id.to_string());
        }
        if let Some(mapping_status) = self.mapping_status {
            filter.insert("mapping_status", mapping_status.as_str());
        }
        filter
    }
}

impl Pagination for ExternalIdentityMapFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ExternalIdentityMap> {
    /// 分页检索外部身份映射列表（投影查询）。
    ///
    /// 只返回 [`ExternalIdentityMapRow`] 所需的列表字段，不加载整文档
    /// （`external_id_key` 二进制字段不进入列表投影）。
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
    pub async fn search_external_identity_maps(
        &self,
        filter: &ExternalIdentityMapFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ExternalIdentityMapRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(external_identity_map_projection())
            .build();
        let collection = self.collection().clone_with_type::<ExternalIdentityMapRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「来源系统 + 对象类型 + 规范化比较键」查找唯一映射。
    ///
    /// 唯一性由 `uk_external_identity_maps_identity` 唯一索引保证；本方法
    /// 用于映射查询与幂等判定，服务层不得做「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `source_system_id` - 来源系统 ID
    /// * `object_type` - 外部对象类型
    /// * `external_id_key` - 规范化比较键（实体 `ExternalIdentityMap::external_id_key` 生成）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除映射；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_identity(
        &self,
        source_system_id: &SourceSystemId,
        object_type: ExternalObjectType,
        external_id_key: &ExternalIdKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<ExternalIdentityMap>> {
        self.find_one(
            doc! {
                "source_system_id": source_system_id.to_string(),
                "object_type": object_type.as_str(),
                "external_id_key": external_id_key.to_bson_binary(),
            },
            executor,
        )
        .await
    }
}

/// D01 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `SourceRegistryExt::source_registry()` 访问。
pub struct SourceRegistryRepository<'a> {
    db: &'a Database,
}

impl<'a> SourceRegistryRepository<'a> {
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

    /// 建立外部身份映射（跨集合多步骤写入）。
    ///
    /// 依次写入 `external_identity_maps` 与 `external_identity_targets`，
    /// 保证「映射身份 + 目标谱系」原子可见（数据模型 §6.1）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有映射没有目标的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `map` - 待写入的外部身份映射
    /// * `target` - 待写入的映射目标（谱系记录）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_external_identity_link(
        &self,
        map: &ExternalIdentityMap,
        target: &ExternalIdentityTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<ExternalIdentityMap>(EXTERNAL_IDENTITY_MAPS),
            map,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self
                .db
                .collection::<ExternalIdentityTarget>(EXTERNAL_IDENTITY_TARGETS),
            target,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { sort_by.unwrap_or("created_at"): direction }
}

/// 来源系统列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn source_system_projection() -> Document {
    doc! {
        "id": 1,
        "code": 1,
        "name": 1,
        "system_type": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 外部身份映射列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn external_identity_map_projection() -> Document {
    doc! {
        "id": 1,
        "source_system_id": 1,
        "object_type": 1,
        "external_id": 1,
        "mapping_status": 1,
        "mapped_at": 1,
        "mapped_by": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, QueryFilter, SourceSystemFilter};
    use mongodb::bson::doc;

    #[test]
    fn source_system_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SourceSystemFilter {
            code: Some("ERP".to_string()),
            system_type: Some(entities::source_registry::SourceSystemType::Mall),
            status: Some(entities::source_registry::SourceSystemStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("code").unwrap(), "ERP");
        assert_eq!(document.get_str("system_type").unwrap(), "MALL");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_descending() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("code"), true), doc! { "code": 1 });
    }
}
