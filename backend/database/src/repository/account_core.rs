//! AccountCore实体的特化方法

use super::Repository;
use crate::errors::Result;
use crate::{mongo_ops, Executor};
use entities::{AccountCore, AccountKind};
use mongodb::bson::doc;

impl<'a> Repository<'a, AccountCore> {
    /// 根据账号查找统一账号。
    ///
    /// # 参数
    /// * `account` - 账号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回匹配的统一账号
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_account(
        &self,
        account: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        self.find_one_by_field("account", account, executor).await
    }

    /// 根据账号查找统一账号，包含已软删除记录。
    ///
    /// 全局唯一索引包含软删除记录；账号占用校验必须使用本方法。
    ///
    /// # 参数
    /// * `account` - 账号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回匹配的统一账号
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_account_including_deleted(
        &self,
        account: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        mongo_ops::find_one(&self.collection(), doc! { "account": account }, executor).await
    }

    /// 根据 ID 查询统一账号，包含已软删除记录。
    ///
    /// # 参数
    /// * `id` - 账号 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回匹配的账号记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id_including_deleted(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        mongo_ops::find_one(&self.collection(), doc! { "id": id }, executor).await
    }

    /// 根据账号类型查询账号集合。
    ///
    /// # 参数
    /// * `kind` - 账号类型
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回匹配的账号集合
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn list_by_kind(
        &self,
        kind: AccountKind,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountCore>> {
        let filter = doc! {
            "kind": kind.as_str(),
        };
        self.find_many(filter, executor).await
    }
}
