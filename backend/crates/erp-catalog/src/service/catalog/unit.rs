use crate::entity::catalog::unit_of_measure::{UnitOfMeasure, UnitOfMeasureUpdate};
use crate::repository::CatalogExt;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::support::ensure_version;
use super::CatalogService;
use crate::dto::{PageView, SortDir, UnitOfMeasureListParams, UnitOfMeasureView, UpdateUnitOfMeasureRequest};
use crate::error::Result;
use application_core::AuditActor;

/// 计量单位列表筛选条件类型。
type UnitOfMeasureFilter = <mongodb::Database as CatalogExt>::UnitOfMeasureFilter;

impl CatalogService {
    // ---------- 计量单位 ----------

    /// 分页查询计量单位列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`unit_code`/`name`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn unit_of_measure_list(
        &self,
        params: &UnitOfMeasureListParams,
    ) -> Result<PageView<UnitOfMeasureView>> {
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
        let page = self
            .db
            .unit_of_measures()
            .search_unit_of_measures(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| UnitOfMeasureView {
                id: row.id,
                unit_code: row.unit_code,
                name: row.name,
                symbol: row.symbol,
                quantity_scale: row.quantity_scale,
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

    /// 更新计量单位（乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 单位 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后单位的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 单位不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn unit_of_measure_update(
        &self,
        id: &str,
        req: UpdateUnitOfMeasureRequest,
        actor: &AuditActor,
    ) -> Result<UnitOfMeasureView> {
        req.validate()?;
        let mut unit = self.load_unit(id).await?;
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
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.unit_of_measures().update(&mut unit, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<UnitOfMeasure, crate::error::Error>(unit)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除计量单位（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 单位 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 单位不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn unit_of_measure_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut unit = self.load_unit(id).await?;
        let audit = self.audit.resource_log(
            actor.clone(),
            "unit_of_measure.delete",
            "unit_of_measure",
            unit.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let audit_port = self.audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.unit_of_measures().soft_delete(&mut unit, session).await?;
                    audit_port.persist(&audit, session).await?;
                    Ok::<(), crate::error::Error>(())
                })
            })
            .await
    }
}
