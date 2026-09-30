//! 人员唯一范围仓储；旧集合只允许迁移入口读取。
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::access_control::person_scope::PersonDataScope;

#[allow(async_fn_in_trait)]
pub trait PersonDataScopeRepositoryExt {
    /// 读取人员全部范围，可按业务及动作限定。
    /// # 参数
    /// 人员、业务动作及调用方执行器。
    /// # 返回
    /// 唯一键稳定排序的配置。
    /// # 错误
    /// 数据库错误原样返回。
    async fn for_person(
        &self,
        user: &str,
        resource: Option<&str>,
        action: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PersonDataScope>>;
}
impl PersonDataScopeRepositoryExt for Repository<'_, PersonDataScope> {
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
