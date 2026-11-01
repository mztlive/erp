//! `background_job_items` 列表投影行与仓储查询。

use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::entity::bulk_job::{BackgroundJobId, BackgroundJobItem, ItemStatus};

/// 后台任务逐项结果投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobItemRow {
    /// 实体主键。
    pub id: String,
    /// 所属后台任务 ID（与查询过滤一致，投影已覆盖）。
    pub background_job_id: String,
    /// 稳定逐项序号。
    pub item_no: u32,
    /// 已有对象类型代码。
    pub object_type: Option<String>,
    /// 已有对象 ID。
    pub object_id: Option<String>,
    /// 导入工作表名。
    pub worksheet_name: Option<String>,
    /// 导入源行号。
    pub source_row_no: Option<u32>,
    /// 逐项执行结果（未执行为 `None`）。
    pub status: Option<ItemStatus>,
    /// 脱敏原因代码。
    pub result_code: Option<String>,
    /// 脱敏结果摘要。
    pub result_summary: Option<String>,
    /// 成功形成的对象类型代码。
    pub result_object_type: Option<String>,
    /// 成功形成的对象 ID。
    pub result_object_id: Option<String>,
}

/// 后台任务逐项集合仓储的域查询。
#[allow(async_fn_in_trait)]
pub trait BackgroundJobItemRepositoryExt {
    /// 分页检索任务逐项结果（投影查询）。
    ///
    /// 只返回 [`BackgroundJobItemRow`] 所需的逐项字段，不加载整文档；
    /// `status` 过滤覆盖 `idx_background_job_items_status`。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    /// * `status` - 逐项执行结果；`None` 表示不筛选
    /// * `page` - 页码（1 起）
    /// * `page_size` - 单页条数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页逐项结果行与总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_job_items(
        &self,
        job_id: &BackgroundJobId,
        status: Option<ItemStatus>,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BackgroundJobItemRow>>;
}

impl BackgroundJobItemRepositoryExt for persistence_core::Repository<'_, BackgroundJobItem> {
    async fn search_job_items(
        &self,
        job_id: &BackgroundJobId,
        status: Option<ItemStatus>,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BackgroundJobItemRow>> {
        let mut filter = doc! { "background_job_id": job_id.to_string() };
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        let options = FindOptions::builder()
            .sort(doc! { "item_no": 1 })
            .skip((page.max(1) - 1) * u64::from(page_size))
            .limit(i64::from(page_size))
            .projection(job_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<BackgroundJobItemRow>();
        let items = mongo_ops::find_many(&collection, filter.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter, executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }
}

/// 后台任务逐项结果投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn job_item_projection() -> Document {
    doc! {
        "id": 1,
        "background_job_id": 1,
        "item_no": 1,
        "object_type": 1,
        "object_id": 1,
        "worksheet_name": 1,
        "source_row_no": 1,
        "status": 1,
        "result_code": 1,
        "result_summary": 1,
        "result_object_type": 1,
        "result_object_id": 1,
    }
}
