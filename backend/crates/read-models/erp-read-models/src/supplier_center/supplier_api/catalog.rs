//! 连接商品目录、真实后台任务与检查点的组合查询。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::CompiledAccess;
use erp_permissions::internal;
use erp_supply::dto::supplier_catalog::{CatalogFilter, CatalogItemView, CatalogSyncMode};
use erp_supply::ports::connector::offer::IncrementalSupport;
use erp_supply::repository::supplier_api::authorized_connection;
use erp_supply::repository::supplier_catalog::{self, SupplierCatalogExt};
use erp_support::repository::prelude::*;
use erp_support::{BulkJobExt, JobStatus};
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use super::SupplierApiReadService;
use crate::{Error, Result};

#[derive(Deserialize)]
pub struct CatalogQuery {
    #[serde(default = "first_page")]
    pub page: u32,
    #[serde(default = "page_size")]
    pub page_size: u32,
    #[serde(flatten)]
    pub filter: CatalogFilter,
}
fn first_page() -> u32 {
    1
}
fn page_size() -> u32 {
    20
}
#[derive(Serialize)]
pub struct CatalogView {
    pub items: Vec<CatalogItemView>,
    pub total: u64,
    pub product_total: u64,
    pub can_read_cost: bool,
    pub page: u32,
    pub page_size: u32,
    pub jobs: Vec<CatalogJobView>,
    pub filters: CatalogFilterOptions,
    pub sync_mode: CatalogSyncMode,
    pub coverage: &'static str,
    pub incremental_supported: bool,
}
#[derive(Serialize)]
pub struct CatalogFilterOptions {
    pub categories: Vec<supplier_catalog::CatalogFilterOption>,
    pub brands: Vec<supplier_catalog::CatalogFilterOption>,
    pub category_hierarchy_supported: bool,
    pub category_hierarchy_reason: &'static str,
}
#[derive(Serialize)]
pub struct CatalogJobView {
    pub job_id: String,
    pub job_no: String,
    pub status: JobStatus,
    pub observed_count: u64,
    pub skipped_count: u64,
    pub skipped_samples: Vec<String>,
    pub pages: u32,
    pub created_at: u64,
    pub finished_at: Option<i64>,
    pub error_summary: Option<String>,
    pub stalled: bool,
    pub sync_mode: CatalogSyncMode,
}
impl SupplierApiReadService {
    /// 读取限定连接的商品分页与最近二十次实际运行结果。
    /// # 参数
    /// id 为授权连接，query 为分页与筛选，actor 为当前账号；不接受供应商身份替换。
    /// # 返回
    /// 同一快照内的来源观察、成本展示资格与真实同步计数。
    /// # 错误
    /// 连接不存在、分页非法或数据库失败时拒绝。
    pub async fn catalog(&self, id: &str, query: CatalogQuery, actor: &AuditActor) -> Result<CatalogView> {
        if query.page == 0 || !(1..=100).contains(&query.page_size) {
            return Err(Error::ValidationError("商品分页参数无效".into()));
        }
        query.filter.validate()?;

        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.catalog_snapshot(&id, &query, &actor, executor).await })
            })
            .await
    }

    /// 在同一快照内完成展示资格、根查询及目录全部补齐后再校验交付。
    /// # 参数
    /// id、query 为已校验的连接和分页筛选，actor 为当前账号，executor 为原快照执行器。
    /// # 返回
    /// 返回原查询凭证覆盖的商品、任务及筛选项投影。
    /// # 错误
    /// 执行边界、资格、成本筛选或读取失败时拒绝；最终凭证失效时不交付。
    pub(super) async fn catalog_snapshot(
        &self,
        id: &str,
        query: &CatalogQuery,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CatalogView> {
        let (access, can_read_cost) = self.catalog_access(actor, executor).await?;
        let connection = authorized_connection(&self.db, id, &access, executor).await?;
        query.filter.ensure_cost_access(can_read_cost)?;

        let (items, total, product_total) = self.catalog_items(id, query, can_read_cost, executor).await?;
        let jobs = self.catalog_jobs(id, executor).await?;
        let filters = self.catalog_filters(id, executor).await?;
        let sync_mode = jobs.first().map_or(CatalogSyncMode::Full, |job| job.sync_mode);

        let support = self
            .gateway
            .as_ref()
            .map(|gateway| gateway.catalog_policy(&connection))
            .transpose()
            .map_err(|error| Error::BusinessLogicError(error.summary))?
            .flatten()
            .map_or(IncrementalSupport::Unsupported, |policy| policy.incremental);

        let view = CatalogView {
            items,
            total,
            product_total,
            can_read_cost,
            page: query.page,
            page_size: query.page_size,
            jobs,
            filters,
            sync_mode,
            coverage: if support == IncrementalSupport::Complete { "COMPLETE" } else { "PARTIAL" },
            incremental_supported: support != IncrementalSupport::Unsupported,
        };

        self.authorization.validate(&access, executor).await?;
        Ok(view)
    }

    /// 成本展示资格先于最终目录查询，后续读取不再开启授权阶段。
    /// # 参数
    /// actor 为当前账号，executor 必须属于活动快照事务。
    /// # 返回
    /// 返回最终目录查询凭证与本快照的成本展示资格。
    /// # 错误
    /// 执行边界、展示资格取证或目录查询资格失败时拒绝。
    pub(super) async fn catalog_access(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(CompiledAccess, bool)> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("供应连接目录读取必须使用快照事务".into()));
        }
        let can_read_cost = self
            .authorization
            .has_permission(actor, &internal::SUPPLIER_OFFERING_COST_DETAIL, executor)
            .await?;
        let access = self.authorization.query(actor, &internal::CONNECTION_CATALOG_READ, executor).await?;
        Ok((access, can_read_cost))
    }

    async fn catalog_items(
        &self,
        id: &str,
        query: &CatalogQuery,
        can_read_cost: bool,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<CatalogItemView>, u64, u64)> {
        let (mut items, total) = supplier_catalog::filtered_page(
            &self.db,
            id,
            &query.filter,
            query.page,
            query.page_size,
            executor,
        )
        .await?;
        let counts = supplier_catalog::filtered_counts(&self.db, id, &query.filter, executor).await?;

        if !can_read_cost {
            for item in &mut items {
                item.purchase_price = None;
                item.source_fingerprint.clear();
            }
        }
        Ok((items.into_iter().map(CatalogItemView::from).collect(), total, counts.product_total))
    }

    async fn catalog_filters(&self, id: &str, executor: &mut dyn Executor) -> Result<CatalogFilterOptions> {
        Ok(CatalogFilterOptions {
            categories: supplier_catalog::filter_options(&self.db, id, true, executor).await?,
            brands: supplier_catalog::filter_options(&self.db, id, false, executor).await?,
            category_hierarchy_supported: false,
            category_hierarchy_reason: "来源未提供可靠分类层级，暂不支持包含子分类",
        })
    }

    async fn catalog_jobs(&self, id: &str, executor: &mut dyn Executor) -> Result<Vec<CatalogJobView>> {
        let records = self.db.background_jobs().recent_catalog_jobs(id, executor).await?;
        let mut jobs = Vec::with_capacity(records.len());
        for job in records {
            let run = self.db.supplier_catalog_runs().find_by_id(&job.base.id, executor).await?;
            let stalled = job.catalog_stalled(Instant::now());
            jobs.push(CatalogJobView {
                stalled,
                sync_mode: run.as_ref().map_or(CatalogSyncMode::Full, |run| run.sync_mode),
                job_id: job.base.id,
                job_no: job.job_no,
                status: job.status,
                observed_count: run.as_ref().map_or(0, |v| v.observed_count),
                skipped_count: run.as_ref().map_or(0, |v| v.skipped_count),
                skipped_samples: run.as_ref().map_or_else(Vec::new, |v| v.skipped_samples.clone()),
                pages: run.as_ref().map_or(0, |v| v.pages),
                created_at: job.base.created_at,
                finished_at: job.finished_at.map(|at| at.unix_secs()),
                error_summary: job.error_summary,
            });
        }
        Ok(jobs)
    }
}

#[cfg(test)]
mod tests {
    use erp_supply::dto::supplier_catalog::CatalogSelectionStatus;

    use super::CatalogQuery;

    #[test]
    fn catalog_query_preserves_exact_filter_values_and_rejects_invalid_range() {
        let query: CatalogQuery = serde_json::from_value(serde_json::json!({"page":2,"page_size":20,"keyword":"蛋糕","price_min":"0.1001","price_max":"1.0000","selection_status":"UNSELECTED"})).unwrap();
        assert_eq!(query.page, 2);
        assert_eq!(query.filter.price_min.unwrap().to_string(), "0.1001");
        assert_eq!(query.filter.selection_status, Some(CatalogSelectionStatus::Unselected));
        query.filter.validate().unwrap();

        let invalid: CatalogQuery =
            serde_json::from_value(serde_json::json!({"price_min":"2","price_max":"1"})).unwrap();
        assert!(invalid.filter.validate().is_err());
    }
}
