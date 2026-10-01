//! 查询当前操作人的商品导入任务与逐项结果。

use std::collections::{HashMap, HashSet};

use application_core::{AuditActor, PageView};
use erp_catalog::{
    ProductImportItemListParams, ProductImportItemView, ProductImportJobListParams, ProductImportJobView,
};
use erp_core::ids::{BackgroundJobId, FileAssetId};
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BackgroundJobFilter, BulkJobExt, FileAsset, FileAssetExt, JobType,
    PRODUCT_IMPORT_DOMAIN_JOB_TYPE,
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
        let ids = page.items.iter().map(|row| BackgroundJobId::new(row.id.clone())).collect::<Vec<_>>();
        let jobs = self.db.background_jobs().find_by_ids(&ids, &mut NoTransaction).await?;
        let files = self.db.file_assets().find_by_ids(&source_file_ids(&jobs), &mut NoTransaction).await?;
        let items = page_job_views(&ids, &jobs, &files)?;
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

    /// 读取单任务并保留领域类型与发起人校验。
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

/// 按任务事实收集去重源文件 ID，缺少源文件的任务不触发读取。
fn source_file_ids(jobs: &[BackgroundJob]) -> Vec<FileAssetId> {
    let mut seen = HashSet::new();
    jobs.iter()
        .filter_map(|job| job.input_file_asset_id.as_ref())
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect()
}

/// 按原分页顺序组装最新事实；任务消失保持报错，文件消失保持空名称。
fn page_job_views(
    ids: &[BackgroundJobId],
    jobs: &[BackgroundJob],
    files: &[FileAsset],
) -> Result<Vec<ProductImportJobView>> {
    let jobs = jobs.iter().map(|job| (job.base.id.as_str(), job)).collect::<HashMap<_, _>>();
    let files = files.iter().map(|file| (file.base.id.as_str(), file)).collect::<HashMap<_, _>>();
    ids.iter()
        .map(|id| {
            let job = jobs.get(id.as_ref()).ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
            let file = job.input_file_asset_id.as_ref().and_then(|id| files.get(id.as_ref()).copied());
            Ok(job_view(job, file_name_of(file)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use erp_support::{BackgroundJobData, RegisterFileAssetRequest, RetentionClass, SensitivityClass};

    use super::{
        BackgroundJob, BackgroundJobId, Error, FileAsset, FileAssetId, JobType, page_job_views,
        source_file_ids,
    };

    /// 构造内存任务，模拟分页后批量回读的最新事实。
    fn job(id: &str, file: Option<&str>) -> BackgroundJob {
        BackgroundJob::new(
            BackgroundJobId::new(id),
            BackgroundJobData {
                job_no: format!("IMPORT-{id}"),
                job_type: JobType::Import,
                domain_job_type: Some("PRODUCT_IMPORT".into()),
                domain_job_id: None,
                selection_snapshot_id: None,
                requested_by: "admin".into(),
                request_id: format!("request-{id}"),
                input_file_asset_id: file.map(FileAssetId::new),
                result_file_asset_id: None,
                total_count: 3,
            },
        )
        .unwrap()
    }

    /// 构造可展示名称的文件事实，不访问对象存储。
    fn file(id: &str, name: &str) -> FileAsset {
        let registration = RegisterFileAssetRequest {
            storage_object_key: format!("import/{id}"),
            file_name: name.into(),
            content_type: "application/octet-stream".into(),
            byte_size: 1,
            content_hmac: "a".repeat(64),
            sensitivity_class: SensitivityClass::General,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        };
        FileAsset::new(FileAssetId::new(id), registration.into_data("admin").unwrap()).unwrap()
    }

    /// 批量结果无序仍按页顺序展示最新进度，重复源文件只读取一次。
    #[test]
    fn page_views_restore_order_latest_progress_and_shared_file() {
        let mut first = job("first", Some("shared"));
        first.base.version = 9;
        first.processed_count = 2;
        first.success_count = 2;
        let second = job("second", Some("shared"));
        let jobs = vec![second, first];
        assert_eq!(source_file_ids(&jobs), vec![FileAssetId::new("shared")]);
        let views = page_job_views(
            &[BackgroundJobId::new("first"), BackgroundJobId::new("second")],
            &jobs,
            &[file("shared", "报价表.xlsx")],
        )
        .unwrap();
        assert_eq!(views.iter().map(|view| view.id.as_str()).collect::<Vec<_>>(), ["first", "second"]);
        assert_eq!(views[0].version, 9);
        assert_eq!(views[0].processed_count, 2);
        assert!(views.iter().all(|view| view.file_name.as_deref() == Some("报价表.xlsx")));
    }

    /// 文件消失与无源文件保持空名称，任务消失保持原 NotFound。
    #[test]
    fn page_views_preserve_missing_relationship_semantics_and_empty_pages() {
        let jobs = [job("missing-file", Some("removed")), job("no-file", None)];
        let ids = jobs.iter().map(|job| BackgroundJobId::new(&job.base.id)).collect::<Vec<_>>();
        let views = page_job_views(&ids, &jobs, &[]).unwrap();
        assert!(views.iter().all(|view| view.file_name.is_none()));
        assert!(page_job_views(&[], &[], &[]).unwrap().is_empty());
        assert!(source_file_ids(&[]).is_empty());
        assert!(matches!(
            page_job_views(&[BackgroundJobId::new("removed")], &jobs, &[]),
            Err(Error::NotFound(message)) if message == "导入任务不存在"
        ));
    }
}
