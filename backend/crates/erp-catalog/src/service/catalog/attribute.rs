use application_core::AuditActor;
use persistence_core::{NoTransaction, Transactional};
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
use crate::repository::CatalogExt;

/// 规格属性列表筛选条件类型。
type SkuAttributeFilter = <mongodb::Database as CatalogExt>::SkuAttributeFilter;
/// 规格属性值列表筛选条件类型。
type SkuAttributeValueFilter = <mongodb::Database as CatalogExt>::SkuAttributeValueFilter;

impl CatalogService {
    // ---------- 规格属性 ----------

    /// 分页查询规格属性列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`attribute_code`/`name`/`value_type`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_attribute_list(
        &self,
        params: &SkuAttributeListParams,
    ) -> Result<PageView<SkuAttributeView>> {
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
        let page = self.db.sku_attributes().search_sku_attributes(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(SkuAttributeView::from).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 更新规格属性（乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 属性 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后属性的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 属性不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn sku_attribute_update(
        &self,
        id: &str,
        req: UpdateSkuAttributeRequest,
        actor: &AuditActor,
    ) -> Result<SkuAttributeView> {
        req.validate()?;
        let mut attribute = self.load_attribute(id).await?;
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
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sku_attributes().update(&mut attribute, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<SkuAttribute, crate::error::Error>(attribute)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除规格属性（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 属性 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 属性不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn sku_attribute_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut attribute = self.load_attribute(id).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "sku_attribute.delete",
            "sku_attribute",
            attribute.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sku_attributes().soft_delete(&mut attribute, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await
    }

    // ---------- 规格属性值 ----------

    /// 分页查询规格属性值列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`attribute_id`/`value_code`/`display_value`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_attribute_value_list(
        &self,
        params: &SkuAttributeValueListParams,
    ) -> Result<PageView<SkuAttributeValueView>> {
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
        let page =
            self.db.sku_attribute_values().search_sku_attribute_values(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(SkuAttributeValueView::from).collect();
        Ok(PageView { items, total: page.total, page: filter.page, page_size: filter.page_size })
    }

    /// 更新规格属性值（乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 属性值 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后属性值的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 属性值不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn sku_attribute_value_update(
        &self,
        id: &str,
        req: UpdateSkuAttributeValueRequest,
        actor: &AuditActor,
    ) -> Result<SkuAttributeValueView> {
        req.validate()?;
        let mut value = self.load_attribute_value(id).await?;
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
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sku_attribute_values().update(&mut value, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<SkuAttributeValue, crate::error::Error>(value)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除规格属性值（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 属性值 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 属性值不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn sku_attribute_value_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut value = self.load_attribute_value(id).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "sku_attribute_value.delete",
            "sku_attribute_value",
            value.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sku_attribute_values().soft_delete(&mut value, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await
    }

    /// 按 ID 加载未删除规格属性。
    ///
    /// # 参数
    /// * `id` - 属性 ID
    ///
    /// # 返回
    /// 返回属性实体。
    ///
    /// # 错误
    /// 属性不存在时返回 `NotFound`。
    pub async fn load_attribute(&self, id: &str) -> Result<SkuAttribute> {
        self.db
            .sku_attributes()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("规格属性不存在".to_string()))
    }

    /// 按 ID 加载未删除规格属性值。
    ///
    /// # 参数
    /// * `id` - 属性值 ID
    ///
    /// # 返回
    /// 返回属性值实体。
    ///
    /// # 错误
    /// 属性值不存在时返回 `NotFound`。
    async fn load_attribute_value(&self, id: &str) -> Result<SkuAttributeValue> {
        self.db
            .sku_attribute_values()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("规格属性值不存在".to_string()))
    }
}
