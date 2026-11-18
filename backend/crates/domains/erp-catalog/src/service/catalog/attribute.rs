use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::catalog;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::CatalogService;
use super::support::ensure_version;
use crate::dto::{
    PageView, SkuAttributeListParams, SkuAttributeValueListParams, SkuAttributeValueView, SkuAttributeView,
    SortDir, UpdateSkuAttributeRequest, UpdateSkuAttributeValueRequest,
};
use crate::entity::catalog::sku_attribute::{SkuAttribute, SkuAttributeUpdate};
use crate::entity::catalog::sku_attribute_value::{SkuAttributeValue, SkuAttributeValueUpdate};
use crate::error::{Error, Result};
use crate::repository::prelude::*;
use crate::repository::{CatalogExt, SkuAttributeFilter, SkuAttributeValueFilter};

impl CatalogService {
    // ---------- 规格属性 ----------

    /// 分页查询规格属性列表。
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    ///
    /// `params` 为查询参数（`attribute_code`/`name`/`value_type`/`status` 扁平筛选）。
    pub async fn sku_attribute_list(
        &self,
        actor: &AuditActor,
        params: &SkuAttributeListParams,
    ) -> Result<PageView<SkuAttributeView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.sku_attribute_list_snapshot(&actor, &params, executor).await })
            })
            .await
    }

    // 分页查询SKU 规格属性，沿用公开入口提供的授权快照完成查询。
    async fn sku_attribute_list_snapshot(
        &self,
        actor: &AuditActor,
        params: &SkuAttributeListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SkuAttributeView>> {
        self.data_scope
            .authorization
            .require(actor, &catalog::SKU_ATTRIBUTE_LIST, AccessTarget::Operation, executor)
            .await?;

        params.validate()?;
        let query = params.normalized()?;
        let filter = SkuAttributeFilter {
            attribute_code: query.attribute_code,
            name: query.name,
            value_type: query.value_type,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };

        let page = self.db.sku_attributes().search_sku_attributes(&filter, executor).await?;
        let items = page.items.into_iter().map(SkuAttributeView::from).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 更新规格属性（乐观锁语义）。
    ///
    /// 失败情形如下：
    /// * `NotFound` - 属性不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    ///
    /// `req` 为更新请求（含期望版本）。
    pub async fn sku_attribute_update(
        &self,
        id: &str,
        req: UpdateSkuAttributeRequest,
        actor: &AuditActor,
    ) -> Result<SkuAttributeView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.sku_attribute_update_stage(&id, req, &actor, executor).await })
            })
            .await
    }

    // 更新SKU 规格属性，在原事务内完成授权、关联事实核对和写入与审计。
    async fn sku_attribute_update_stage(
        &self,
        id: &str,
        req: UpdateSkuAttributeRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SkuAttributeView> {
        self.data_scope
            .authorization
            .require(actor, &catalog::SKU_ATTRIBUTE_UPDATE, AccessTarget::Operation, executor)
            .await?;

        req.validate()?;
        let mut attribute = self.load_attribute(id, executor).await?;
        ensure_version(attribute.base.version, req.version)?;
        attribute.update(
            SkuAttributeUpdate { name: req.name, value_type: req.value_type, status: req.status },
            actor.id(),
        )?;

        let audit = self.audit.resource_log(
            actor.clone(),
            "sku_attribute.update",
            "sku_attribute",
            attribute.base.id.clone(),
        )?;
        self.db.sku_attributes().update(&mut attribute, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(attribute.into())
    }

    /// 删除规格属性（软删除，乐观锁语义）。
    ///
    /// 失败情形如下：
    /// * `NotFound` - 属性不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn sku_attribute_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.sku_attribute_delete_stage(&id, &actor, executor).await })
            })
            .await
    }

    // 删除SKU 规格属性，在原事务内完成授权、关联事实核对和写入与审计。
    async fn sku_attribute_delete_stage(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.data_scope
            .authorization
            .require(actor, &catalog::SKU_ATTRIBUTE_DELETE, AccessTarget::Operation, executor)
            .await?;

        let mut attribute = self.load_attribute(id, executor).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "sku_attribute.delete",
            "sku_attribute",
            attribute.base.id.clone(),
        )?;
        self.db.sku_attributes().soft_delete(&mut attribute, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(())
    }

    // ---------- 规格属性值 ----------

    /// 分页查询规格属性值列表。
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    ///
    /// `params` 为查询参数（`attribute_id`/`value_code`/`display_value`/`status` 扁平筛选）。
    pub async fn sku_attribute_value_list(
        &self,
        actor: &AuditActor,
        params: &SkuAttributeValueListParams,
    ) -> Result<PageView<SkuAttributeValueView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.sku_attribute_value_list_snapshot(&actor, &params, executor).await },
                )
            })
            .await
    }

    // 分页查询SKU 规格属性值，沿用公开入口提供的授权快照完成查询。
    async fn sku_attribute_value_list_snapshot(
        &self,
        actor: &AuditActor,
        params: &SkuAttributeValueListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SkuAttributeValueView>> {
        self.data_scope
            .authorization
            .require(actor, &catalog::SKU_ATTRIBUTE_VALUE_LIST, AccessTarget::Operation, executor)
            .await?;

        params.validate()?;
        let query = params.normalized()?;
        let filter = SkuAttributeValueFilter {
            attribute_id: query.attribute_id,
            value_code: query.value_code,
            display_value: query.display_value,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };

        let page = self.db.sku_attribute_values().search_sku_attribute_values(&filter, executor).await?;
        let items = page.items.into_iter().map(SkuAttributeValueView::from).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 更新规格属性值（乐观锁语义）。
    ///
    /// 失败情形如下：
    /// * `NotFound` - 属性值不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    ///
    /// `req` 为更新请求（含期望版本）。
    pub async fn sku_attribute_value_update(
        &self,
        id: &str,
        req: UpdateSkuAttributeValueRequest,
        actor: &AuditActor,
    ) -> Result<SkuAttributeValueView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.sku_attribute_value_update_stage(&id, req, &actor, executor).await },
                )
            })
            .await
    }

    // 更新SKU 规格属性值，在原事务内完成授权、关联事实核对和写入与审计。
    async fn sku_attribute_value_update_stage(
        &self,
        id: &str,
        req: UpdateSkuAttributeValueRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SkuAttributeValueView> {
        self.data_scope
            .authorization
            .require(actor, &catalog::SKU_ATTRIBUTE_VALUE_UPDATE, AccessTarget::Operation, executor)
            .await?;

        req.validate()?;
        let mut value = self.load_attribute_value(id, executor).await?;
        ensure_version(value.base.version, req.version)?;
        value.update(
            SkuAttributeValueUpdate {
                display_value: req.display_value,
                sort_order: req.sort_order,
                status: req.status,
            },
            actor.id(),
        )?;

        let audit = self.audit.resource_log(
            actor.clone(),
            "sku_attribute_value.update",
            "sku_attribute_value",
            value.base.id.clone(),
        )?;
        self.db.sku_attribute_values().update(&mut value, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(value.into())
    }

    /// 删除规格属性值（软删除，乐观锁语义）。
    ///
    /// 失败情形如下：
    /// * `NotFound` - 属性值不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn sku_attribute_value_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.sku_attribute_value_delete_stage(&id, &actor, executor).await })
            })
            .await
    }

    // 删除SKU 规格属性值，在原事务内完成授权、关联事实核对和写入与审计。
    async fn sku_attribute_value_delete_stage(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.data_scope
            .authorization
            .require(actor, &catalog::SKU_ATTRIBUTE_VALUE_DELETE, AccessTarget::Operation, executor)
            .await?;

        let mut value = self.load_attribute_value(id, executor).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "sku_attribute_value.delete",
            "sku_attribute_value",
            value.base.id.clone(),
        )?;
        self.db.sku_attribute_values().soft_delete(&mut value, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(())
    }

    /// 按 ID 加载未删除规格属性。属性不存在时返回 `NotFound`。
    ///
    /// 使用调用方读取或写入事务的原执行器。
    pub async fn load_attribute(&self, id: &str, executor: &mut dyn Executor) -> Result<SkuAttribute> {
        self.db
            .sku_attributes()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("规格属性不存在".to_string()))
    }

    // 按 ID 加载未删除规格属性值。属性值不存在时返回 `NotFound`。
    async fn load_attribute_value(&self, id: &str, executor: &mut dyn Executor) -> Result<SkuAttributeValue> {
        self.db
            .sku_attribute_values()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("规格属性值不存在".to_string()))
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
                    .sku_attribute_update_stage(
                        "missing",
                        serde_json::from_value(json!({"version":version})).unwrap(),
                        &actor,
                        &mut executor,
                    )
                    .await,
            );
            denied(
                service
                    .sku_attribute_value_update_stage(
                        "missing",
                        serde_json::from_value(json!({"version":version})).unwrap(),
                        &actor,
                        &mut executor,
                    )
                    .await,
            );

            denied(service.sku_attribute_delete_stage("missing", &actor, &mut executor).await);
            denied(service.sku_attribute_value_delete_stage("missing", &actor, &mut executor).await);
        }
    }
}
