//! 人员查询资格的集合访问。

use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::person_directory::{PersonDirectoryCategory, PersonQueryQualification};

/// 人员查询资格仓储扩展。
#[allow(async_fn_in_trait)]
pub trait PersonQueryQualificationRepositoryExt {
    /// 读取某账号在指定类别上的未删除资格。
    ///
    /// # 参数
    /// * `account_id` - 账号 ID
    /// * `category` - 查询类别
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回资格记录；不存在时返回 `None`。
    ///
    /// # 错误
    /// 查询失败时返回仓储错误。
    async fn find_for_account(
        &self,
        account_id: &str,
        category: PersonDirectoryCategory,
        executor: &mut dyn Executor,
    ) -> Result<Option<PersonQueryQualification>>;
}

impl PersonQueryQualificationRepositoryExt for Repository<'_, PersonQueryQualification> {
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
