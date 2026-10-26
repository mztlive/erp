//! 连接商品目录、真实后台任务与检查点的组合查询。
use erp_core::common::time::Instant;
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
}
#[derive(Serialize)]
pub struct CatalogJobView {
    pub job_id: String,
    pub job_no: String,
    pub status: JobStatus,
    pub observed_count: u64,
    pub pages: u32,
    pub created_at: u64,
    pub finished_at: Option<i64>,
    pub error_summary: Option<String>,
    pub stalled: bool,
}
impl SupplierApiReadService {
    /// 读取限定连接的商品分页与最近二十次实际运行结果。
    /// # 参数
    /// `id` 为授权连接，`query` 为分页；不接受供应商身份替换。
    /// # 返回
    /// 来源观察与真实同步计数。
    /// # 错误
    /// 连接不存在、分页非法或数据库失败时拒绝。
    pub async fn catalog(&self, id: &str, query: CatalogQuery) -> Result<CatalogView> {
        if query.page == 0 || !(1..=100).contains(&query.page_size) {
            return Err(Error::ValidationError("商品分页参数无效".into()));
        }
        self.domain().load_connection(id, &mut NoTransaction).await?;
        let (items, total) =
            supplier_catalog::page(&self.db, id, query.page, query.page_size, &mut NoTransaction).await?;
        let records = self.db.background_jobs().recent_catalog_jobs(id, &mut NoTransaction).await?;
        let mut jobs = Vec::with_capacity(records.len());
        for job in records {
            let run = self.db.supplier_catalog_runs().find_by_id(&job.base.id, &mut NoTransaction).await?;
            let stalled = job.catalog_stalled(Instant::now());
            jobs.push(CatalogJobView {
                stalled,
                job_id: job.base.id,
                job_no: job.job_no,
                status: job.status,
                observed_count: run.as_ref().map_or(0, |v| v.observed_count),
                pages: run.as_ref().map_or(0, |v| v.pages),
                created_at: job.base.created_at,
                finished_at: job.finished_at.map(|at| at.unix_secs()),
                error_summary: job.error_summary,
            });
        }
        Ok(CatalogView { items, total, page: query.page, page_size: query.page_size, jobs })
    }
}
