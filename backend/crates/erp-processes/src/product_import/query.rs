//! 查询当前操作人的商品导入任务与逐项结果。

use application_core::{AuditActor, PageView};
use erp_catalog::{
    ProductImportItemListParams, ProductImportItemView, ProductImportJobListParams, ProductImportJobView,
};
use erp_core::ids::BackgroundJobId;
use erp_support::{
    BackgroundJob, BackgroundJobFilter, BulkJobExt, FileAssetExt, JobType, PRODUCT_IMPORT_DOMAIN_JOB_TYPE,
};
use persistence_core::NoTransaction;
use validator::Validate;

use super::ProductImportProcess;
use super::views::{file_name_of, item_view, job_view};
use crate::{Error, Result};

impl ProductImportProcess {
    /// 查询当前操作人发起的商品导入任务。
    ///
    /// # 参数
    /// * `params` - 分页参数
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回任务分页。
    ///
    /// # 错误
    /// 分页非法或查询失败时返回错误。
    pub async fn job_list(
        &self,
        params: &ProductImportJobListParams,
        actor: &AuditActor,
    ) -> Result<PageView<ProductImportJobView>> {
        params.validate()?;
        let paging = params.paging();
        let filter = BackgroundJobFilter {
            job_no: None,
            job_type: Some(JobType::Import),
            domain_job_type: Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE.to_string()),
            status: None,
            requested_by: Some(actor.id().to_string()),
            page: paging.page,
            page_size: paging.page_size,
            sort_by: Some(paging.sort_by.to_string()),
            sort_ascending: false,
        };
        let page = self.db.background_jobs().search_background_jobs(&filter, &mut NoTransaction).await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let job = self
                .db
                .background_jobs()
                .find_by_id(&row.id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
            let file = match &job.input_file_asset_id {
                Some(id) => self.db.file_assets().find_by_id(id.as_ref(), &mut NoTransaction).await?,
                None => None,
            };
            items.push(job_view(&job, file_name_of(file.as_ref())));
        }
        Ok(PageView { items, total: page.total, page: paging.page, page_size: paging.page_size })
    }

    /// 查询单个导入任务详情。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回任务视图。
    ///
    /// # 错误
    /// 任务不存在或不属于当前操作人时返回错误。
    pub async fn job_detail(&self, id: &str, actor: &AuditActor) -> Result<ProductImportJobView> {
        let job = self.load_owned_job(id, actor).await?;
        let file = match &job.input_file_asset_id {
            Some(file_id) => self.db.file_assets().find_by_id(file_id.as_ref(), &mut NoTransaction).await?,
            None => None,
        };
        Ok(job_view(&job, file_name_of(file.as_ref())))
    }

    /// 查询导入任务逐项结果。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `params` - 分页参数
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回逐项分页。
    ///
    /// # 错误
    /// 任务不存在或不属于当前操作人时返回错误。
    pub async fn job_items(
        &self,
        id: &str,
        params: &ProductImportItemListParams,
        actor: &AuditActor,
    ) -> Result<PageView<ProductImportItemView>> {
        params.validate()?;
        self.load_owned_job(id, actor).await?;
        let (page, page_size) = params.paging();
        let result = self
            .db
            .background_job_items()
            .search_job_items(&BackgroundJobId::new(id), None, page, page_size, &mut NoTransaction)
            .await?;
        Ok(PageView {
            items: result.items.into_iter().map(item_view).collect(),
            total: result.total,
            page,
            page_size,
        })
    }

    async fn load_owned_job(&self, id: &str, actor: &AuditActor) -> Result<BackgroundJob> {
        let job = self
            .db
            .background_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
        if job.domain_job_type.as_deref() != Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE) {
            return Err(Error::NotFound("导入任务不存在".into()));
        }
        if job.requested_by != actor.id() {
            return Err(Error::Forbidden("只能查看自己提交的导入任务".into()));
        }
        Ok(job)
    }
}
