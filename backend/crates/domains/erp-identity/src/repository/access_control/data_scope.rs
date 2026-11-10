//! 域 D06 `access_control` 仓储：data_scope 范围查询。

use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Repository, Result};

use crate::entity::access_control::{DataScope, DataScopeSubjectType};

/// 数据范围集合仓储的域特有查询。
#[allow(async_fn_in_trait)]
pub trait DataScopeRepositoryExt {
    /// 检测未迁移的个人上限；旧个人规则不能因 v2 过滤而丢失收窄效果。
    ///
    /// # 参数
    /// * `user_id` - 当前账号 ID
    /// * `executor` - 原事务执行器
    /// # 返回
    /// 存在未软删除且非 v2 的个人规则时为 true。
    /// # 错误
    /// 数据库错误传播，不转换为无上限。
    async fn has_legacy_user_limit(&self, user_id: &str, executor: &mut dyn Executor) -> Result<bool>;

    /// 按单个主体取回数据范围。
    ///
    /// # 参数
    /// * `subject_type` - 范围主体类型
    /// * `subject_id` - 范围主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该主体全部未软删除且 `schema_version` 为 2 的数据范围。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_by_subject(
        &self,
        subject_type: DataScopeSubjectType,
        subject_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DataScope>>;

    /// 按同类主体 ID 集合批量取回数据范围。
    ///
    /// 按 `subject_type` 与 `subject_id` 批量读取。Repository 只返回
    /// `schema_version` 为 2 且未软删除的持久化事实，不计算用户与角色范围的授权交集。
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
    async fn list_by_subjects(
        &self,
        subject_type: DataScopeSubjectType,
        subject_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<DataScope>>;
}

impl DataScopeRepositoryExt for Repository<'_, DataScope> {
    async fn has_legacy_user_limit(&self, user_id: &str, executor: &mut dyn Executor) -> Result<bool> {
        self.exists(
            doc! { "subject_type": "user", "subject_id": user_id, "schema_version": { "$ne": 2 } },
            executor,
        )
        .await
    }

    async fn list_by_subject(
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

    async fn list_by_subjects(
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
