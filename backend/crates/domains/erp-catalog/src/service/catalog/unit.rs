use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::catalog;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CatalogService;
use super::support::ensure_version;
use crate::dto::{PageView, SortDir, UnitOfMeasureListParams, UnitOfMeasureView, UpdateUnitOfMeasureRequest};
use crate::entity::catalog::unit_of_measure::UnitOfMeasureUpdate;
use crate::error::Result;
use crate::repository::prelude::*;
use crate::repository::{CatalogExt, UnitOfMeasureFilter};

impl CatalogService {
    // ---------- 计量单位 ----------

    /// 分页查询计量单位列表。
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    ///
    /// `params` 为查询参数（`unit_code`/`name`/`status` 扁平筛选）。
    pub async fn unit_of_measure_list(
        &self,
        actor: &AuditActor,
        params: &UnitOfMeasureListParams,
    ) -> Result<PageView<UnitOfMeasureView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.unit_of_measure_list_snapshot(&actor, &params, executor).await },
                )
            })
            .await
    }

    // 分页查询计量单位，沿用公开入口提供的授权快照完成查询。
    async fn unit_of_measure_list_snapshot(
        &self,
        actor: &AuditActor,
        params: &UnitOfMeasureListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<UnitOfMeasureView>> {
        self.data_scope
            .authorization
            .require(actor, &catalog::UNIT_OF_MEASURE_LIST, AccessTarget::Operation, executor)
            .await?;

        params.validate()?;
        let query = params.normalized()?;
        let filter = UnitOfMeasureFilter {
            unit_code: query.unit_code,
            name: query.name,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };

        let page = self.db.unit_of_measures().search_unit_of_measures(&filter, executor).await?;
        let items = page.items.into_iter().map(UnitOfMeasureView::from).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 更新计量单位（乐观锁语义）。
    ///
    /// 失败情形如下：
    /// * `NotFound` - 单位不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    ///
    /// `req` 为更新请求（含期望版本）。
    pub async fn unit_of_measure_update(
        &self,
        id: &str,
        req: UpdateUnitOfMeasureRequest,
        actor: &AuditActor,
    ) -> Result<UnitOfMeasureView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.unit_of_measure_update_stage(&id, req, &actor, executor).await },
                )
            })
            .await
    }

    // 更新计量单位，在原事务内完成授权、关联事实核对和写入与审计。
    async fn unit_of_measure_update_stage(
        &self,
        id: &str,
        req: UpdateUnitOfMeasureRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<UnitOfMeasureView> {
        self.data_scope
            .authorization
            .require(actor, &catalog::UNIT_OF_MEASURE_UPDATE, AccessTarget::Operation, executor)
            .await?;

        req.validate()?;
        let mut unit = self.load_unit(id, executor).await?;
        ensure_version(unit.base.version, req.version)?;
        unit.update(
            UnitOfMeasureUpdate {
                name: req.name,
                symbol: req.symbol,
                quantity_scale: req.quantity_scale,
                status: req.status,
            },
            actor.id(),
        )?;

        let audit = self.audit.resource_log(
            actor.clone(),
            "unit_of_measure.update",
            "unit_of_measure",
            unit.base.id.clone(),
        )?;
        self.db.unit_of_measures().update(&mut unit, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(unit.into())
    }

    /// 删除计量单位（软删除，乐观锁语义）。
    ///
    /// 失败情形如下：
    /// * `NotFound` - 单位不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn unit_of_measure_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.unit_of_measure_delete_stage(&id, &actor, executor).await })
            })
            .await
    }

    // 删除计量单位，在原事务内完成授权、关联事实核对和写入与审计。
    async fn unit_of_measure_delete_stage(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.data_scope
            .authorization
            .require(actor, &catalog::UNIT_OF_MEASURE_DELETE, AccessTarget::Operation, executor)
            .await?;

        let mut unit = self.load_unit(id, executor).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "unit_of_measure.delete",
            "unit_of_measure",
            unit.base.id.clone(),
        )?;
        self.db.unit_of_measures().soft_delete(&mut unit, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::super::testing::{actor, denied, service};

    #[tokio::test]
    async fn writes_authorize_before_existing_records_versions_or_related_facts() {
        let service = service().await;
        let actor = actor();
        let mut executor = SnapshotExecutor::default();
        for version in [1, 999] {
            denied(
                service
                    .unit_of_measure_update_stage(
                        "missing",
                        serde_json::from_value(json!({"version":version})).unwrap(),
                        &actor,
                        &mut executor,
                    )
                    .await,
            );
            denied(service.unit_of_measure_delete_stage("missing", &actor, &mut executor).await);
        }
    }
}
