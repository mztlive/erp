//! 人员唯一范围仓储；旧集合只允许迁移入口读取。
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::access_control::person_scope::PersonDataScope;

/// 为当前人员数据范围集合提供按人查询，不读取旧范围集合。
#[allow(async_fn_in_trait)]
pub trait PersonDataScopeRepositoryExt {
    /// 读取指定人员的未删除范围，按资源和动作稳定排序。资源与动作均可省略，省略时不限制对应维度；数据库错误原样返回。
    async fn for_person(
        &self,
        user: &str,
        resource: Option<&str>,
        action: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PersonDataScope>>;
}
impl PersonDataScopeRepositoryExt for Repository<'_, PersonDataScope> {
    // 将人员与可选资源、动作条件下推到数据库，再按资源和动作排序。
    async fn for_person(
        &self,
        user: &str,
        resource: Option<&str>,
        action: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PersonDataScope>> {
        let mut filter = doc! { "user_id": user };
        if let Some(resource) = resource {
            filter.insert("resource", resource);
        }
        if let Some(action) = action {
            filter.insert("action", action);
        }

        self.find_many_sorted(filter, doc! { "resource": 1, "action": 1 }, executor).await
    }
}
