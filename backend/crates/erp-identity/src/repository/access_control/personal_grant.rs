//! 个人业务授权按人员及业务读取，查询细节不泄露到应用服务。
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::access_control::personal_grant::PersonalBusinessGrant;

#[allow(async_fn_in_trait)]
pub trait PersonalBusinessGrantRepositoryExt {
    /// 读取指定人员未撤销的附加授权，可限定当前业务。
    /// # 参数
    /// * `user_id` - 固定人员。
    /// * `resource` - 可选明确业务。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 按创建时间、ID 稳定排序的授权。
    /// # 错误
    /// 持久化错误原样返回。
    async fn for_person(
        &self,
        user_id: &str,
        resource: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PersonalBusinessGrant>>;
}

impl PersonalBusinessGrantRepositoryExt for Repository<'_, PersonalBusinessGrant> {
    async fn for_person(
        &self,
        user_id: &str,
        resource: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PersonalBusinessGrant>> {
        let mut filter = doc! { "user_id": user_id };
        if let Some(resource) = resource {
            filter.insert("resource", resource);
        }
        self.find_many_sorted(filter, doc! { "created_at": 1, "id": 1 }, executor).await
    }
}
