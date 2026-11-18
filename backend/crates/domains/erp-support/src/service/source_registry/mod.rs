//! 来源系统与外部身份映射入口执行统一功能授权；写入与审计共用原事务。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::source_registry;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::check_expected_version;
pub use crate::dto::source_registry::{
    CreateExternalIdentityMapRequest, CreateSourceSystemRequest, ExternalIdentityMapListParams,
    ExternalIdentityMapView, PageView, SourceSystemListParams, SourceSystemView, UpdateSourceSystemRequest,
};
use crate::entity::source_registry::{
    ExternalIdentityMap, ExternalIdentityMapData, ExternalIdentityMapId, ExternalIdentityTarget,
    ExternalIdentityTargetData, ExternalIdentityTargetId, MappingStatus, SourceSystemUpdate, TargetStatus,
};
use crate::error::{Error, Result};
use crate::ports::SupportAuditPort;
use crate::repository::SourceRegistryExt;
use crate::repository::prelude::*;

/// 来源系统列表筛选条件类型（经 `SourceRegistryExt` 关联类型跨 crate 可达）。
type SourceSystemFilter = <mongodb::Database as SourceRegistryExt>::SourceSystemFilter;
/// 外部身份映射列表筛选条件类型。
type ExternalIdentityMapFilter = <mongodb::Database as SourceRegistryExt>::ExternalIdentityMapFilter;

/// 来源注册服务。
///
/// 提供来源系统与外部身份映射的创建、查询与更新编排。
#[derive(Clone)]
pub struct SourceRegistryService {
    db: Database,
    audit: Arc<dyn SupportAuditPort>,
    authorization: Arc<AuthorizationService>,
}

impl SourceRegistryService {
    /// 创建来源注册服务实例。
    /// `audit` 为审计写入端口；`authorization` 为进程共享的统一授权服务。
    pub fn new(
        db: Database,
        audit: Arc<dyn SupportAuditPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, audit, authorization }
    }

    /// 分页查询来源系统列表。
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    /// `params` 为查询参数。
    ///
    /// 失败条件：
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn source_system_list(
        &self,
        params: &SourceSystemListParams,
        actor: &AuditActor,
    ) -> Result<PageView<SourceSystemView>> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.source_system_page(&params, &actor, executor).await })
            })
            .await
    }

    // 功能资格、分页及计数共用原快照事务。
    async fn source_system_page(
        &self,
        params: &SourceSystemListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SourceSystemView>> {
        params.validate()?;
        self.authorization
            .require(actor, &source_registry::SOURCE_SYSTEM_LIST, AccessTarget::Operation, executor)
            .await?;

        let query = params.normalized()?;
        let (page, page_size, sort_by, sort_ascending) = query.paging.into_filter_parts();
        let filter = SourceSystemFilter {
            code: query.code,
            system_type: query.system_type,
            status: query.status,
            page,
            page_size,
            sort_by,
            sort_ascending,
        };

        let page = self.db.source_systems().search_source_systems(&filter, executor).await?;
        let items = page.items.into_iter().map(SourceSystemView::from).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 更新来源系统（乐观锁语义）。
    /// 期望版本 `req.version` 与当前版本不一致时直接返回冲突（409）；仓储层 `Repository::update` 同时以 `id + version` CAS 兜底并发竞争（base.rs：`OptimisticLockingError` → 服务层 `ConflictError`）。
    /// 返回更新后来源系统的响应视图。
    /// `id` 为来源系统 ID；`req` 为更新请求（含期望版本）。
    ///
    /// 失败条件：
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

        let (db, audit_port, authorization) =
            (self.db.clone(), self.audit.clone(), self.authorization.clone());
        let (id, actor) = (id.to_string(), actor.clone());
        let updated = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(
                            &actor,
                            &source_registry::SOURCE_SYSTEM_UPDATE,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;

                    let mut system = db
                        .source_systems()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源系统不存在".into()))?;
                    check_expected_version(system.base.version, req.version)?;
                    system.update(SourceSystemUpdate { name: req.name, status: req.status }, actor.id())?;

                    let audit = audit_port.resource_log(
                        actor,
                        "source_system.update",
                        "source_system",
                        system.base.id.clone(),
                    )?;
                    db.source_systems().update(&mut system, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<_, Error>(system)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 建立外部身份映射（跨集合事务写入）。
    /// 在一个事务内写入 `external_identity_maps`、`external_identity_targets` 与审计日志，保证「映射身份 + 目标谱系」原子可见（数据模型 §6.1）。
    /// 唯一性冲突由唯一索引透出 `DuplicateKey` → `ConflictError`，不做应用层「先查后插」的重复性判断。
    /// 返回新建映射的响应视图。
    ///
    /// 失败条件：
    /// * `NotFound` - 来源系统不存在
    /// * `ConflictError` - 同一 (来源系统, 对象类型, 比较键) 已存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_external_identity_map(
        &self,
        req: CreateExternalIdentityMapRequest,
        actor: &AuditActor,
    ) -> Result<ExternalIdentityMapView> {
        req.validate()?;

        let (db, audit_port, authorization) =
            (self.db.clone(), self.audit.clone(), self.authorization.clone());
        let actor = actor.clone();
        let map = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(
                            &actor,
                            &source_registry::EXTERNAL_IDENTITY_MAP_CREATE,
                            AccessTarget::Operation,
                            executor,
                        )
                        .await?;

                    db.source_systems()
                        .find_by_id(&req.source_system_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源系统不存在".into()))?;

                    let (map, target) = build_identity_link(req)?;
                    let audit = audit_port.resource_log(
                        actor,
                        "external_identity_map.create",
                        "external_identity_map",
                        map.base.id.clone(),
                    )?;
                    db.source_registry().create_external_identity_link(&map, &target, executor).await?;
                    audit_port.persist(&audit, executor).await?;
                    Ok::<_, Error>(map)
                })
            })
            .await?;
        Ok(map.into())
    }

    /// 分页查询外部身份映射列表。
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    /// `params` 为查询参数（`source_system_id`/`mapping_status` 扁平筛选）。
    ///
    /// 失败条件：
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn external_identity_map_list(
        &self,
        params: &ExternalIdentityMapListParams,
        actor: &AuditActor,
    ) -> Result<PageView<ExternalIdentityMapView>> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.external_identity_page(&params, &actor, executor).await })
            })
            .await
    }

    // 功能资格、外部身份映射分页及计数共用原快照事务。
    async fn external_identity_page(
        &self,
        params: &ExternalIdentityMapListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PageView<ExternalIdentityMapView>> {
        params.validate()?;
        self.authorization
            .require(actor, &source_registry::EXTERNAL_IDENTITY_MAP_LIST, AccessTarget::Operation, executor)
            .await?;

        let query = params.normalized()?;
        let (page, page_size, sort_by, sort_ascending) = query.paging.into_filter_parts();
        let filter = ExternalIdentityMapFilter {
            source_system_id: query.source_system_id,
            mapping_status: query.mapping_status,
            page,
            page_size,
            sort_by,
            sort_ascending,
        };

        let page = self.db.external_identity_maps().search_external_identity_maps(&filter, executor).await?;
        let items = page.items.into_iter().map(ExternalIdentityMapView::from).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }
}

// 由创建请求组装外部身份映射与目标谱系两实体（初始状态均为待处理）。
// 返回待写入同一事务的映射与目标实体。
// `req` 为创建请求（含来源系统、外部身份与内部对象指向）。
// 任一实体校验失败时返回错误。
fn build_identity_link(
    req: CreateExternalIdentityMapRequest,
) -> Result<(ExternalIdentityMap, ExternalIdentityTarget)> {
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
    Ok((map, target))
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::FailClosedAuditPort;

    #[tokio::test]
    async fn registry_reads_require_identity_before_loading_systems_or_external_maps() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service = SourceRegistryService::new(
            client.database("source_registry_authorization_test"),
            Arc::new(FailClosedAuditPort),
            unavailable_authorization(),
        );
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let maps = serde_json::from_value::<ExternalIdentityMapListParams>(serde_json::json!({})).unwrap();

        let errors = [
            service
                .source_system_page(
                    &SourceSystemListParams::default(),
                    &actor,
                    &mut SnapshotExecutor::default(),
                )
                .await
                .expect_err("必须拒绝"),
            service
                .external_identity_page(&maps, &actor, &mut SnapshotExecutor::default())
                .await
                .expect_err("必须拒绝"),
        ];
        for error in errors {
            assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
        }
    }
}
