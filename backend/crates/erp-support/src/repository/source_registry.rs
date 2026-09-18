//! 域 D01 `source_registry` 仓储：source_system、external_identity_map、external_identity_target。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`persistence_core::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名直接引用 `extensions::SourceRegistryExt`
//! 关联常量（唯一来源，conventions §4.3），不做本地转存。
//!
//! 筛选/行类型定义在本文件，经 `SourceRegistryExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use super::extensions::SourceRegistryExt;
use super::page::{search_projected_page, sort_direction};
use crate::entity::source_registry::{
    ExternalIdKey, ExternalIdentityMap, ExternalIdentityTarget, ExternalObjectType, MappingStatus,
    RelationRole, SourceSystem, SourceSystemId, SourceSystemStatus, SourceSystemType, TargetStatus,
};
use crate::repository::owned::{
    ExternalIdentityMapRepository, ExternalIdentityTargetRepository, SourceSystemRepository,
};

/// Encode an external identity comparison key as BSON Binary (Generic).
///
/// The unique index `uk_external_identity_maps_identity` is built on this
/// binary field. Callers must use this helper instead of entity-level BSON.
pub fn external_id_key_bson(key: &ExternalIdKey) -> mongodb::bson::Binary {
    mongodb::bson::Binary {
        subtype: mongodb::bson::spec::BinarySubtype::Generic,
        bytes: key.as_bytes().to_vec(),
    }
}

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

impl Default for SourceSystemFilter {
    /// 缺省分页从第一页、每页二十条开始，其余筛选保持空条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回第 1 页、每页 20 条的空筛选条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            code: None,
            system_type: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
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

impl<'a> SourceSystemRepository<'a> {
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
        search_projected_page(&self.collection(), &collection, filter, options, executor).await
    }

    /// 按来源系统 ID 集合批量读取来源系统（INT-R17）。
    ///
    /// 一次 `$in` 查询装载页面所需的全部来源商城；空输入不访问数据库。
    ///
    /// # 参数
    /// * `source_system_ids` - 来源系统 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的来源系统；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只返回实体，不返回 services DTO、HTTP View 或授权结论。
    pub async fn find_systems_by_ids(
        &self,
        source_system_ids: &[SourceSystemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SourceSystem>> {
        if source_system_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = source_system_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
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

impl<'a> ExternalIdentityMapRepository<'a> {
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
        search_projected_page(&self.collection(), &collection, filter, options, executor).await
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
                "external_id_key": external_id_key_bson(external_id_key),
            },
            executor,
        )
        .await
    }

    /// 按页面来源身份集合批量读取外部身份映射（INT-R17）。
    ///
    /// 一次 `$or` 查询装载本页全部任务的谱系映射；空输入不访问数据库。返回
    /// 顺序不承诺与输入一致，调用方按来源身份归组。元组项为
    /// （来源系统，对象类型，规范化比较键）。
    ///
    /// # 参数
    /// * `lookups` - 本页任务的来源身份集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的映射；无谱系的身份不出现。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只返回实体，不返回 services DTO、HTTP View 或授权结论。
    pub async fn find_maps_by_identities(
        &self,
        lookups: &[(SourceSystemId, ExternalObjectType, ExternalIdKey)],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ExternalIdentityMap>> {
        if lookups.is_empty() {
            return Ok(Vec::new());
        }
        let alternatives = lookups
            .iter()
            .map(|(source_system_id, object_type, external_id_key)| {
                doc! {
                    "source_system_id": source_system_id.to_string(),
                    "object_type": object_type.as_str(),
                    "external_id_key": external_id_key_bson(external_id_key),
                }
            })
            .collect::<Vec<_>>();
        self.find_many(doc! { "$or": alternatives }, executor).await
    }
}

impl<'a> ExternalIdentityTargetRepository<'a> {
    /// 查询外部身份映射的全部目标历史，最新有效期优先。
    ///
    /// # 参数
    /// * `mapping_id` - 外部身份映射 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部目标历史，按 `valid_from` 降序、ID 升序稳定排列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `external_identity_targets` 集合，不访问映射集合。
    pub async fn list_for_external_identity_map(
        &self,
        mapping_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ExternalIdentityTarget>> {
        self.find_many_sorted(
            doc! { "external_identity_map_id": mapping_id },
            doc! { "valid_from": -1, "id": 1 },
            executor,
        )
        .await
    }

    /// 查询外部身份映射当前有效目标，按生效时间稳定排序。
    ///
    /// # 参数
    /// * `mapping_id` - 外部身份映射 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回状态为 `Active` 的目标，按 `valid_from` 与 ID 升序排列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `external_identity_targets` 集合，不访问映射集合。
    pub async fn list_active_for_external_identity_map(
        &self,
        mapping_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ExternalIdentityTarget>> {
        self.find_many_sorted(
            doc! {
                "external_identity_map_id": mapping_id,
                "status": TargetStatus::Active.as_str(),
            },
            doc! { "valid_from": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 在调用方执行器下批量 CAS 过期谱系目标（INT-R24）。
    ///
    /// 逐目标复用基类 `update` 的 `id + version` 乐观锁；全部调用共享同一个
    /// 调用方执行器，Service 在事务内调用时任一冲突随事务整体回滚。版本冲突
    /// 的目标 ID 收集为类型化结果，不抛 services DTO、HTTP View 或授权结论。
    ///
    /// # 参数
    /// * `targets` - 已调用 `expire` 的待写入目标（可变，成功时版本递增）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回批量 CAS 结果：`applied` 为成功目标 ID，`conflicts` 为版本冲突目标
    /// ID；空输入不访问数据库并返回双空结果。
    ///
    /// # 错误
    /// 非版本冲突的 MongoDB 写入失败时返回错误；版本冲突只进入 `conflicts`。
    ///
    /// # 约束
    /// 不开事务、不提交事务；调用方必须在 `conflicts` 非空时失败关闭并回滚。
    pub async fn expire_targets_batch(
        &self,
        targets: &mut [ExternalIdentityTarget],
        executor: &mut dyn Executor,
    ) -> Result<ExpireTargetsOutcome> {
        let mut outcome = ExpireTargetsOutcome::default();
        for target in targets.iter_mut() {
            let target_id = target.base.id.clone();
            match self.update(target, executor).await {
                Ok(()) => outcome.applied.push(target_id),
                Err(persistence_core::Error::OptimisticLockingError) => {
                    outcome.conflicts.push(target_id);
                },
                Err(error) => return Err(error),
            }
        }
        Ok(outcome)
    }

    /// 按映射 ID 集合批量加载谱系目标历史（INT-R17）。
    ///
    /// 一次 `$in` 查询装载本页全部谱系的目标历史，按映射归组由 Service 解释；
    /// 返回按 `valid_from` 降序、ID 升序稳定排列。
    ///
    /// # 参数
    /// * `mapping_ids` - 外部身份映射 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的目标历史；无目标的映射不出现。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `external_identity_targets` 集合，不访问映射集合。
    pub async fn list_targets_for_maps(
        &self,
        mapping_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ExternalIdentityTarget>> {
        if mapping_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! { "external_identity_map_id": { "$in": mapping_ids } },
            doc! { "valid_from": -1, "id": 1 },
            executor,
        )
        .await
    }
}

/// 批量过期谱系目标的 CAS 结果（INT-R24 类型化报告）。
///
/// # 约束
/// 存储无关投影，不携带 services DTO、HTTP View 或授权结论。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExpireTargetsOutcome {
    /// CAS 成功并已递增版本的目标 ID。
    pub applied: Vec<String>,
    /// 版本冲突未写入的目标 ID；调用方必须整体回滚。
    pub conflicts: Vec<String>,
}

impl ExpireTargetsOutcome {
    /// 是否存在版本冲突。
    ///
    /// # 返回
    /// 存在任一冲突目标时返回 `true`。
    ///
    /// # 错误
    /// 无错误返回。
    ///
    /// # 约束
    /// 纯状态判断；调用方必须在 `true` 时失败关闭并回滚事务。
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
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

    /// 反向解析指定商城下某个 ERP 规范对象的当前有效外部身份。
    ///
    /// 只接受 `PRIMARY + ACTIVE` 且有效期覆盖 `as_of_unix_secs` 的目标谱系，随后
    /// 关联同一来源系统、同一对象类型且状态为 `MAPPED` 的映射。返回全部命中，
    /// 由 Service 强制“恰好一条”；这样零条与多条不会被仓储静默猜测。
    ///
    /// # 参数
    /// * `source_system_id` - 目标商城来源系统
    /// * `object_type` - 客户或卡券类目
    /// * `internal_object_id` - ERP 规范对象 ID
    /// * `as_of_unix_secs` - 提交时服务端 Unix 秒
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 按目标谱系逐条返回匹配的外部 ID；重复活动目标不会被去重。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn active_external_identities_for_internal_object(
        &self,
        source_system_id: &SourceSystemId,
        object_type: ExternalObjectType,
        internal_object_id: &str,
        as_of_unix_secs: i64,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let targets = mongo_ops::find_many(
            &self.db.collection::<ExternalIdentityTarget>(
                <Database as SourceRegistryExt>::EXTERNAL_IDENTITY_TARGETS,
            ),
            active_identity_target_filter(object_type, internal_object_id, as_of_unix_secs),
            FindOptions::builder().sort(doc! { "id": 1 }).build(),
            executor,
        )
        .await?;
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let map_ids =
            targets.iter().map(|target| target.external_identity_map_id.to_string()).collect::<Vec<_>>();
        let maps = mongo_ops::find_many(
            &self
                .db
                .collection::<ExternalIdentityMap>(<Database as SourceRegistryExt>::EXTERNAL_IDENTITY_MAPS),
            active_identity_map_filter(source_system_id, object_type, map_ids),
            FindOptions::builder().sort(doc! { "id": 1 }).build(),
            executor,
        )
        .await?;
        let external_ids =
            maps.into_iter().map(|mapping| (mapping.base.id, mapping.external_id)).collect::<HashMap<_, _>>();

        Ok(targets
            .into_iter()
            .filter_map(|target| external_ids.get(target.external_identity_map_id.as_ref()).cloned())
            .collect())
    }

    /// 建立外部身份映射（跨集合多步骤写入）。
    ///
    /// 依次写入 `external_identity_maps` 与 `external_identity_targets`，
    /// 保证「映射身份 + 目标谱系」原子可见（数据模型 §6.1）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有映射没有目标的半成品；
    /// Service 必须通过 `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `map` - 待写入的外部身份映射
    /// * `target` - 待写入的映射目标（谱系记录）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_external_identity_link(
        &self,
        map: &ExternalIdentityMap,
        target: &ExternalIdentityTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<ExternalIdentityMap>(<Database as SourceRegistryExt>::EXTERNAL_IDENTITY_MAPS),
            map,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<ExternalIdentityTarget>(
                <Database as SourceRegistryExt>::EXTERNAL_IDENTITY_TARGETS,
            ),
            target,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构造商城外部身份反向解析的活动目标过滤条件。
fn active_identity_target_filter(
    object_type: ExternalObjectType,
    internal_object_id: &str,
    as_of_unix_secs: i64,
) -> Document {
    doc! {
        "internal_object_type": object_type.as_str(),
        "internal_object_id": internal_object_id,
        "relation_role": RelationRole::Primary.as_str(),
        "status": TargetStatus::Active.as_str(),
        "valid_from": { "$lte": as_of_unix_secs },
        "$or": [
            { "valid_to": null },
            { "valid_to": { "$exists": false } },
            { "valid_to": { "$gt": as_of_unix_secs } },
        ],
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构造商城外部身份反向解析的已确认映射过滤条件。
fn active_identity_map_filter(
    source_system_id: &SourceSystemId,
    object_type: ExternalObjectType,
    map_ids: Vec<String>,
) -> Document {
    doc! {
        "id": { "$in": map_ids },
        "source_system_id": source_system_id.to_string(),
        "object_type": object_type.as_str(),
        "mapping_status": MappingStatus::Mapped.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
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
    doc! { sort_by.unwrap_or("created_at"): sort_direction(sort_ascending) }
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
    use mongodb::bson::doc;
    use persistence_core::QueryFilter;

    use super::{SourceSystemFilter, active_identity_map_filter, active_identity_target_filter, sort_doc};
    use crate::entity::source_registry::{ExternalObjectType, SourceSystemId};

    #[test]
    fn source_system_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SourceSystemFilter {
            code: Some("ERP".to_string()),
            system_type: Some(crate::entity::source_registry::SourceSystemType::Mall),
            status: Some(crate::entity::source_registry::SourceSystemStatus::Active),
            ..Default::default()
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

    #[test]
    fn active_identity_filters_require_primary_active_mapped_lineage() {
        let target = active_identity_target_filter(ExternalObjectType::Customer, "customer-1", 100);
        assert_eq!(target.get_str("internal_object_type").unwrap(), "customer");
        assert_eq!(target.get_str("internal_object_id").unwrap(), "customer-1");
        assert_eq!(target.get_str("relation_role").unwrap(), "PRIMARY");
        assert_eq!(target.get_str("status").unwrap(), "active");
        assert_eq!(target.get_document("valid_from").unwrap(), &doc! { "$lte": 100_i64 });
        assert!(target.get_array("$or").is_ok());

        let maps = active_identity_map_filter(
            &SourceSystemId::new("mall-1"),
            ExternalObjectType::Customer,
            vec!["map-1".to_string()],
        );
        assert_eq!(maps.get_str("source_system_id").unwrap(), "mall-1");
        assert_eq!(maps.get_str("object_type").unwrap(), "customer");
        assert_eq!(maps.get_str("mapping_status").unwrap(), "mapped");
        assert_eq!(maps.get_document("id").unwrap(), &doc! { "$in": ["map-1"] });
    }

    #[test]
    fn bson_wire_roundtrip_persists_external_id_key_as_binary() {
        use erp_core::ids::{ExternalIdentityMapId, SourceSystemId};
        use mongodb::bson;

        use crate::entity::source_registry::{
            ExternalIdentityMap, ExternalIdentityMapData, ExternalObjectType, MappingStatus,
        };

        let map = ExternalIdentityMap::new(
            ExternalIdentityMapId::new("map-1"),
            ExternalIdentityMapData {
                source_system_id: SourceSystemId::new("sys-1"),
                object_type: ExternalObjectType::SalesOrder,
                external_id: " SO-2025-001 ".to_string(),
                mapping_status: MappingStatus::Pending,
                mapped_at: None,
                mapped_by: None,
            },
        )
        .unwrap();
        let bytes = bson::serialize_to_vec(&map).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        let stored = wire_doc.get("external_id_key").unwrap();
        let bson::Bson::Binary(binary) = stored else {
            panic!("external_id_key 必须以 BSON Binary 持久化，实际为 {stored:?}");
        };
        assert_eq!(binary.bytes, b"SO-2025-001");
        let back: ExternalIdentityMap = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back, map);
    }

    #[test]
    fn entities_roundtrip_through_bson_including_ids() {
        use erp_core::ids::{ExternalIdentityMapId, ExternalIdentityTargetId, SourceSystemId};
        use mongodb::bson;

        use crate::entity::source_registry::{
            ExternalIdentityTarget, ExternalIdentityTargetData, ExternalObjectType, RelationRole,
            SourceSystem, SourceSystemData, SourceSystemStatus, SourceSystemType, TargetStatus,
        };

        let system = SourceSystem::new(
            SourceSystemId::new("sys-1"),
            SourceSystemData {
                code: "ERP".to_string(),
                system_type: SourceSystemType::Mall,
                name: "目标商城".to_string(),
                status: SourceSystemStatus::Active,
            },
            "admin-1",
        )
        .unwrap();
        let roundtrip: SourceSystem =
            bson::deserialize_from_document(bson::serialize_to_document(&system).unwrap()).unwrap();
        assert_eq!(roundtrip, system);

        let target = ExternalIdentityTarget::new(
            ExternalIdentityTargetId::new("target-1"),
            ExternalIdentityTargetData {
                external_identity_map_id: ExternalIdentityMapId::new("map-1"),
                internal_object_type: ExternalObjectType::Sku,
                internal_object_id: "sku-1".to_string(),
                relation_role: RelationRole::Component,
                valid_from: 1_700_000_000,
                valid_to: None,
                status: TargetStatus::Pending,
                approved_at: None,
                approved_by: None,
            },
        )
        .unwrap();
        let roundtrip: ExternalIdentityTarget =
            bson::deserialize_from_document(bson::serialize_to_document(&target).unwrap()).unwrap();
        assert_eq!(roundtrip, target);
    }
}
