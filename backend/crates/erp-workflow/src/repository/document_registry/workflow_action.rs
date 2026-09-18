//! 工作流动作集合仓储：列表投影与按单据历史。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::sort_doc;
use crate::entity::document_registry::{BusinessDocumentId, WorkflowAction, WorkflowActionType};

/// 工作流动作列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowActionRow {
    /// 实体主键。
    pub id: String,
    /// 业务单据 ID。
    pub document_id: String,
    /// 动作类型。
    pub action_type: WorkflowActionType,
    /// 迁移前状态代码。
    pub from_status: String,
    /// 迁移后状态代码。
    pub to_status: String,
    /// 实际操作者。
    pub actor_id: String,
    /// 动作发生时的责任角色。
    pub actor_role: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 工作流动作列表筛选条件。
#[derive(Debug, Clone)]
pub struct WorkflowActionFilter {
    /// 业务单据 ID；`None` 表示不筛选。
    pub document_id: Option<BusinessDocumentId>,
    /// 操作者（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub actor_id: Option<String>,
    /// 动作类型；`None` 表示不筛选。
    pub action_type: Option<WorkflowActionType>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for WorkflowActionFilter {
    fn default() -> Self {
        Self {
            document_id: None,
            actor_id: None,
            action_type: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for WorkflowActionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(document_id) = &self.document_id {
            filter.insert("document_id", document_id.to_string());
        }
        insert_literal_regex_filter(&mut filter, "actor_id", self.actor_id.as_deref());
        if let Some(action_type) = self.action_type {
            filter.insert("action_type", action_type.as_str());
        }
        filter
    }
}

impl Pagination for WorkflowActionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 工作流动作集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait WorkflowActionRepositoryExt {
    /// 分页检索工作流动作（投影查询）。
    ///
    /// 只返回 [`WorkflowActionRow`] 所需的列表字段，不加载整文档；
    /// `actor_id` 按字面量忽略大小写模糊匹配（复用 `repository::regex_filter`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_workflow_actions(
        &self,
        filter: &WorkflowActionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WorkflowActionRow>>;

    /// 按单据查询动作历史（`idx_workflow_actions_document_created`）。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按时间倒序排列的动作历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_by_document(
        &self,
        document_id: &BusinessDocumentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkflowAction>>;
}

impl WorkflowActionRepositoryExt for Repository<'_, WorkflowAction> {
    async fn search_workflow_actions(
        &self,
        filter: &WorkflowActionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WorkflowActionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(workflow_action_projection())
            .build();
        let collection = self.collection().clone_with_type::<WorkflowActionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    async fn list_by_document(
        &self,
        document_id: &BusinessDocumentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkflowAction>> {
        self.find_many_sorted(
            doc! { "document_id": document_id.to_string() },
            doc! { "created_at": -1 },
            executor,
        )
        .await
    }
}

/// 工作流动作列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn workflow_action_projection() -> Document {
    doc! {
        "id": 1,
        "document_id": 1,
        "action_type": 1,
        "from_status": 1,
        "to_status": 1,
        "actor_id": 1,
        "actor_role": 1,
        "created_at": 1,
    }
}
