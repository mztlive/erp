//! 域 D06 `access_control` 仓储：data_scope 范围查询。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use super::{data_scope_projection, sort_doc};
use crate::entity::access_control::{DataScope, DataScopeSubjectType, DataScopeType};
use crate::repository::owned::DataScopeRepository;
/// 数据范围列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataScopeRow {
    /// 版本 2 资源动作绑定。
    #[serde(flatten)]
    pub binding: crate::access_control::ScopeBinding,
    /// 实体主键。
    pub id: String,
    /// 范围主体类型。
    pub subject_type: DataScopeSubjectType,
    /// 范围主体 ID。
    pub subject_id: String,
    /// 范围类型。
    pub scope_type: DataScopeType,
    /// 范围对象。
    pub scope_targets: Vec<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 数据范围列表筛选条件。
#[derive(Debug, Clone)]
pub struct DataScopeFilter {
    /// 范围主体类型；`None` 表示不筛选。
    pub subject_type: Option<DataScopeSubjectType>,
    /// 范围主体 ID；`None` 表示不筛选。
    pub subject_id: Option<String>,
    /// 范围类型；`None` 表示不筛选。
    pub scope_type: Option<DataScopeType>,
    /// 资源；`None` 表示不筛选。
    pub resource: Option<String>,
    /// 动作；`None` 表示不筛选，匹配 `actions` 数组包含该值。
    pub action: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for DataScopeFilter {
    /// 返回首页空筛选（`page: 1`，`page_size: 20`）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回筛选为空、降序的首页过滤条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            subject_type: None,
            subject_id: None,
            scope_type: None,
            resource: None,
            action: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for DataScopeFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回查询条件文档。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 资源按字段精确匹配；动作匹配 `actions` 数组包含该标识，禁止通配。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "schema_version": 2 };
        if let Some(subject_type) = self.subject_type {
            filter.insert("subject_type", subject_type.as_str());
        }
        if let Some(subject_id) = &self.subject_id {
            filter.insert("subject_id", subject_id);
        }
        if let Some(scope_type) = self.scope_type {
            filter.insert("scope_type", scope_type.as_str());
        }
        if let Some(resource) = &self.resource {
            filter.insert("resource", resource);
        }
        if let Some(action) = &self.action {
            filter.insert("actions", action);
        }
        filter
    }
}

impl Pagination for DataScopeFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> DataScopeRepository<'a> {
    /// 查询主体资源的全部配置留痕，包含撤销记录，防止初始化恢复授权。
    ///
    /// # 错误
    /// 底层读取失败时返回仓储错误。
    pub async fn has_subject_resource_history(
        &self,
        role_id: &str,
        resource: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let collection = self
            .database()
            .collection::<Document>(<mongodb::Database as crate::AccessControlExt>::DATA_SCOPES);
        Ok(persistence_core::mongo_ops::find_one(
            &collection,
            doc! { "subject_type": "role", "subject_id": role_id, "resource": resource },
            executor,
        )
        .await?
        .is_some())
    }

    /// 判断指定主体是否存在至少一个未软删除的数据范围。
    ///
    /// 查询只投影 `_id` 并在首条命中后停止，不反序列化完整范围集合。
    ///
    /// # 参数
    /// * `subject_type` - 范围主体类型
    /// * `subject_id` - 范围主体 ID
    /// * `executor` - 调用方事务或非事务执行器
    ///
    /// # 返回
    /// 存在至少一个活跃范围时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn exists_by_subject(
        &self,
        subject_type: DataScopeSubjectType,
        subject_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.exists(
            doc! {
                "subject_type": subject_type.as_str(),
                "subject_id": subject_id,
            },
            executor,
        )
        .await
    }

    /// 分页检索数据范围列表（投影查询）。
    ///
    /// 只返回 [`DataScopeRow`] 所需的配置字段，不加载整文档。
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
    pub async fn search_data_scopes(
        &self,
        filter: &DataScopeFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<DataScopeRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(data_scope_projection())
            .build();
        let collection = self.collection().clone_with_type::<DataScopeRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    /// 检测未迁移的个人上限；旧个人规则不能因 v2 过滤而丢失收窄效果。
    ///
    /// # 参数
    /// * `user_id` - 当前账号 ID
    /// * `executor` - 原事务执行器
    /// # 返回
    /// 存在未软删除且非 v2 的个人规则时为 true。
    /// # 错误
    /// 数据库错误传播，不转换为无上限。
    pub async fn has_legacy_user_limit(&self, user_id: &str, executor: &mut dyn Executor) -> Result<bool> {
        self.exists(
            doc! { "subject_type": "user", "subject_id": user_id, "schema_version": { "$ne": 2 } },
            executor,
        )
        .await
    }

    /// 按单个主体取回数据范围。
    ///
    /// # 参数
    /// * `subject_type` - 范围主体类型
    /// * `subject_id` - 范围主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该主体的全部数据范围。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_subject(
        &self,
        subject_type: DataScopeSubjectType,
        subject_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DataScope>> {
        self.find_many_sorted(
            doc! {
                "schema_version": 2,
                "subject_type": subject_type.as_str(),
                "subject_id": subject_id,
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 按同类主体 ID 集合批量取回数据范围。
    ///
    /// 查询复用 `uk_data_scopes_subject_scope` 的
    /// `(subject_type, subject_id)` 前缀；Repository 只返回未软删除的
    /// 持久化事实，不计算用户与角色范围的授权交集。
    ///
    /// # 参数
    /// * `subject_type` - 范围主体类型
    /// * `subject_ids` - 同类主体 ID 集合；为空时不访问 MongoDB
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未软删除的数据范围；缺失主体不会补齐结果。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_subjects(
        &self,
        subject_type: DataScopeSubjectType,
        subject_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<DataScope>> {
        let Some(filter) = data_scope_subjects_filter(subject_type, subject_ids) else {
            return Ok(Vec::new());
        };
        self.find_many_sorted(filter, doc! { "subject_id": 1, "created_at": 1 }, executor).await
    }
}

/// 构造同类主体批量查询条件。
///
/// # 参数
/// * `subject_type` - 范围主体类型
/// * `subject_ids` - 同类主体 ID 集合
///
/// # 返回
/// 非空输入返回可使用现有主体复合索引的查询条件；空输入返回 `None`。
///
/// # 错误
/// 无；未删除条件由 [`Repository::find_many_sorted`] 统一追加。
pub fn data_scope_subjects_filter(
    subject_type: DataScopeSubjectType,
    subject_ids: &[String],
) -> Option<Document> {
    if subject_ids.is_empty() {
        return None;
    }
    Some(doc! {
        "schema_version": 2,
                "subject_type": subject_type.as_str(),
        "subject_id": { "$in": subject_ids },
    })
}
