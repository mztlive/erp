//! 人员查询资格的集合访问。

use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::person_directory::{PersonDirectoryCategory, PersonQueryQualification};

/// 为人员目录资格集合提供按账号和类别读取配置的入口。
#[allow(async_fn_in_trait)]
pub trait PersonQueryQualificationRepositoryExt {
    /// 查询指定账号在指定目录类别上的未删除资格；缺失返回 `None`，查询失败直接返回仓储错误。
    async fn find_for_account(
        &self,
        account_id: &str,
        category: PersonDirectoryCategory,
        executor: &mut dyn Executor,
    ) -> Result<Option<PersonQueryQualification>>;
}

impl PersonQueryQualificationRepositoryExt for Repository<'_, PersonQueryQualification> {
    // 将账号 ID 和类别同时下推给仓储查询，保留资格记录的实际状态。
    async fn find_for_account(
        &self,
        account_id: &str,
        category: PersonDirectoryCategory,
        executor: &mut dyn Executor,
    ) -> Result<Option<PersonQueryQualification>> {
        let mut rows = self
            .find_many(doc! { "account_id": account_id, "category": category.as_str() }, executor)
            .await?;
        Ok(rows.pop())
    }
}
