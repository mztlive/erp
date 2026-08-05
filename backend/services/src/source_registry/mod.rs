//! 域 D01 `source_registry` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建来源系统：单集合无跨步骤原子性要求 → `&mut NoTransaction`；
//! - 建立外部身份映射：跨集合（map + target + 审计日志）→
//!   `database::Transactional::with_transaction`。
//!
//! 审计写入参考既有写法（`audit::AuditActor::resource_log` +
//! `AccessControlExt::audit_logs`）；仓库尚无 `run_audited_transaction` 模板，
//! 跨集合审计事务按 TRANSACTIONS.md「基本用法」直接编排在 `with_transaction` 内。

use database::{AccessControlExt, NoTransaction, SourceRegistryExt, Transactional};
use entities::source_registry::{
    ExternalIdentityMap, ExternalIdentityMapData, ExternalIdentityMapId, ExternalIdentityTarget,
    ExternalIdentityTargetData, ExternalIdentityTargetId, MappingStatus, SourceSystem, SourceSystemId,
    SourceSystemUpdate, TargetStatus,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    CreateExternalIdentityMapRequest, CreateSourceSystemRequest, ExternalIdentityMapListParams,
    ExternalIdentityMapView, PageView, SourceSystemListParams, SourceSystemView, UpdateSourceSystemRequest,
};

/// 来源系统列表筛选条件类型（经 `SourceRegistryExt` 关联类型跨 crate 可达）。
type SourceSystemFilter = <mongodb::Database as SourceRegistryExt>::SourceSystemFilter;
/// 外部身份映射列表筛选条件类型。
type ExternalIdentityMapFilter = <mongodb::Database as SourceRegistryExt>::ExternalIdentityMapFilter;

/// 来源注册服务。
///
/// 提供来源系统与外部身份映射的创建、查询与更新编排。
pub struct SourceRegistryService {
    db: Database,
}

impl SourceRegistryService {
    /// 创建来源注册服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 创建来源系统（单集合写入，无事务）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建来源系统的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - code 与既有来源系统重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_source_system(
        &self,
        req: CreateSourceSystemRequest,
        actor: &AuditActor,
    ) -> Result<SourceSystemView> {
        req.validate()?;
        let id = SourceSystemId::new(next_id());
        let system = SourceSystem::new(id, req.into_data(), actor.id())?;
        let audit =
            actor
                .clone()
                .resource_log("source_system.create", "source_system", system.base.id.clone())?;

        // 单集合操作无跨集合原子性要求（conventions §6.1），不开启事务；
        // 审计日志按既有写法独立写入（audit::AuditLogService 同款 NoTransaction 形态）。
        self.db
            .source_systems()
            .create(&system, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        Ok(system.into())
    }

    /// 分页查询来源系统列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn source_system_list(
        &self,
        params: &SourceSystemListParams,
    ) -> Result<PageView<SourceSystemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SourceSystemFilter {
            code: query.code,
            system_type: query.system_type,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .source_systems()
            .search_source_systems(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
        // 此处按字段映射为响应视图，避免把仓储类型泄漏到接口层。
        let items = page
            .items
            .into_iter()
            .map(|row| SourceSystemView {
                id: row.id,
                code: row.code,
                name: row.name,
                system_type: row.system_type,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 更新来源系统（乐观锁语义）。
    ///
    /// 期望版本 `req.version` 与当前版本不一致时直接返回冲突（409）；
    /// 仓储层 `Repository::update` 同时以 `id + version` CAS 兜底并发竞争
    /// （base.rs：`OptimisticLockingError` → 服务层 `ConflictError`）。
    ///
    /// # 参数
    /// * `id` - 来源系统 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后来源系统的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源系统不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 请求体校验失败
    pub async fn update_source_system(
        &self,
        id: &str,
        req: UpdateSourceSystemRequest,
        actor: &AuditActor,
    ) -> Result<SourceSystemView> {
        req.validate()?;
        let mut system = self
            .db
            .source_systems()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源系统不存在".to_string()))?;
        if system.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        system.update(
            SourceSystemUpdate {
                name: req.name,
                status: req.status,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("source_system.update", "source_system", system.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.source_systems().update(&mut system, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SourceSystem, crate::errors::Error>(system)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 建立外部身份映射（跨集合事务写入）。
    ///
    /// 在一个事务内写入 `external_identity_maps`、`external_identity_targets`
    /// 与审计日志，保证「映射身份 + 目标谱系」原子可见（数据模型 §6.1）。
    /// 唯一性冲突由唯一索引透出 `DuplicateKey` → `ConflictError`，
    /// 不做应用层「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建映射的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源系统不存在
    /// * `ConflictError` - 同一 (来源系统, 对象类型, 比较键) 已存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_external_identity_map(
        &self,
        req: CreateExternalIdentityMapRequest,
        actor: &AuditActor,
    ) -> Result<ExternalIdentityMapView> {
        req.validate()?;
        self.db
            .source_systems()
            .find_by_id(&req.source_system_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源系统不存在".to_string()))?;

        let map = ExternalIdentityMap::new(
            ExternalIdentityMapId::new(next_id()),
            ExternalIdentityMapData {
                source_system_id: req.source_system_id,
                object_type: req.object_type,
                external_id: req.external_id,
                mapping_status: MappingStatus::Pending,
                mapped_at: None,
                mapped_by: None,
            },
        )?;
        let target = ExternalIdentityTarget::new(
            ExternalIdentityTargetId::new(next_id()),
            ExternalIdentityTargetData {
                external_identity_map_id: map.base.id.clone().into(),
                internal_object_type: req.internal_object_type,
                internal_object_id: req.internal_object_id,
                relation_role: req.relation_role,
                valid_from: req.valid_from,
                valid_to: req.valid_to,
                status: TargetStatus::Pending,
                approved_at: None,
                approved_by: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "external_identity_map.create",
            "external_identity_map",
            map.base.id.clone(),
        )?;

        // 跨集合写入必须处于同一事务：仓库方法
        // `SourceRegistryRepository::create_external_identity_link` 声明
        // 「必须收到事务执行器」，此处由 Service 开启事务会话传入。
        let db = self.db.clone();
        let client = db.client().clone();
        let map_for_tx = map.clone();
        let target_for_tx = target.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.source_registry()
                        .create_external_identity_link(&map_for_tx, &target_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(map.into())
    }

    /// 分页查询外部身份映射列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`source_system_id`/`mapping_status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn external_identity_map_list(
        &self,
        params: &ExternalIdentityMapListParams,
    ) -> Result<PageView<ExternalIdentityMapView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ExternalIdentityMapFilter {
            source_system_id: query.source_system_id,
            mapping_status: query.mapping_status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .external_identity_maps()
            .search_external_identity_maps(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
        // 此处按字段映射为响应视图，避免把仓储类型泄漏到接口层。
        let items = page
            .items
            .into_iter()
            .map(|row| ExternalIdentityMapView {
                id: row.id,
                source_system_id: row.source_system_id,
                object_type: row.object_type,
                external_id: row.external_id,
                mapping_status: row.mapping_status,
                mapped_at: row.mapped_at,
                mapped_by: row.mapped_by,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }
}
