//! 连接商品目录、真实后台任务与检查点的组合查询。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_supply::dto::supplier_catalog::{CatalogFilter, CatalogSyncMode};
use erp_supply::entity::supplier_api::SupplierConnectorType;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::repository::supplier_catalog::{self, SupplierCatalogExt};
use erp_support::repository::prelude::*;
use erp_support::{BulkJobExt, JobStatus};
use persistence_core::NoTransaction;
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
    pub items: Vec<SupplierCatalogItem>,
    pub total: u64,
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
    /// `id` 为授权连接，`query` 为分页；不接受供应商身份替换。
    /// # 返回
    /// 来源观察与真实同步计数。
    /// # 错误
    /// 连接不存在、分页非法或数据库失败时拒绝。
    pub async fn catalog(&self, id: &str, query: CatalogQuery, actor: &AuditActor) -> Result<CatalogView> {
        if query.page == 0 || !(1..=100).contains(&query.page_size) {
            return Err(Error::ValidationError("商品分页参数无效".into()));
        }
        query.filter.validate()?;
        let can_read_cost = self.has_permission(actor, "supplier_offering_cost:detail").await?;
        if !can_read_cost && (query.filter.price_min.is_some() || query.filter.price_max.is_some()) {
            return Err(Error::Forbidden("当前账号不能按采购报价筛选供应商商品".into()));
        }
        let connection = self.domain().load_connection(id, &mut NoTransaction).await?;
        let (mut items, total) = supplier_catalog::filtered_page(
            &self.db,
            id,
            &query.filter,
            query.page,
            query.page_size,
            &mut NoTransaction,
        )
        .await?;
        if !can_read_cost {
            for item in &mut items {
                item.purchase_price = None;
                item.source_fingerprint.clear();
            }
        }
        let jobs = self.catalog_jobs(id).await?;
        let filters = CatalogFilterOptions {
            categories: supplier_catalog::filter_options(&self.db, id, true, &mut NoTransaction).await?,
            brands: supplier_catalog::filter_options(&self.db, id, false, &mut NoTransaction).await?,
        };
        let sync_mode = jobs.first().map_or(CatalogSyncMode::Full, |job| job.sync_mode);
        Ok(CatalogView {
            items,
            total,
            page: query.page,
            page_size: query.page_size,
            jobs,
            filters,
            sync_mode,
            coverage: "PARTIAL",
            incremental_supported: connection.connector_type == Some(SupplierConnectorType::Dangaoshushu),
        })
    }
    async fn catalog_jobs(&self, id: &str) -> Result<Vec<CatalogJobView>> {
        let records = self.db.background_jobs().recent_catalog_jobs(id, &mut NoTransaction).await?;
        let mut jobs = Vec::with_capacity(records.len());
        for job in records {
            let run = self.db.supplier_catalog_runs().find_by_id(&job.base.id, &mut NoTransaction).await?;
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
