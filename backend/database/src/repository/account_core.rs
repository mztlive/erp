//! AccountCore实体的特化方法

use super::Repository;
use crate::errors::Result;
use entities::{AccountCore, AccountKind};
use mongodb::{bson::doc, ClientSession};

impl<'a> Repository<'a, AccountCore> {
    /// 根据账号查找统一账号。
    ///
    /// # 参数
    /// * `account` - 账号
    ///
    /// # 返回值
    /// 返回匹配的统一账号
    pub async fn find_by_account(&self, account: &str) -> Result<Option<AccountCore>> {
        self.find_one_by_field("account", account).await
    }

    /// 根据账号查找统一账号，包含已软删除记录。
    ///
    /// 全局唯一索引包含软删除记录；账号占用校验必须使用本方法。
    ///
    /// # 参数
    /// * `account` - 账号
    ///
    /// # 返回值
    /// 返回匹配的统一账号
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_account_including_deleted(&self, account: &str) -> Result<Option<AccountCore>> {
        let account = self
            .database()
            .collection::<AccountCore>(self.collection_name())
            .find_one(doc! { "account": account })
            .await?;
        Ok(account)
    }

    /// 根据 ID 查询统一账号，包含已软删除记录。
    ///
    /// # 参数
    /// * `id` - 账号 ID
    ///
    /// # 返回值
    /// 返回匹配的账号记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id_including_deleted(&self, id: &str) -> Result<Option<AccountCore>> {
        let account = self
            .database()
            .collection::<AccountCore>(self.collection_name())
            .find_one(doc! { "id": id })
            .await?;
        Ok(account)
    }

    /// 根据账号类型查询账号集合。
    ///
    /// # 参数
    /// * `kind` - 账号类型
    ///
    /// # 返回值
    /// 返回匹配的账号集合
    pub async fn list_by_kind(&self, kind: AccountKind) -> Result<Vec<AccountCore>> {
        let filter = doc! {
            "kind": kind.as_str(),
        };
        self.find_many(filter).await
    }

    /// 在事务中根据 ID 查询账号，包含已软删除记录。
    ///
    /// # 参数
    /// * `id` - 账号 ID
    /// * `session` - 已启动事务的 MongoDB 会话
    ///
    /// # 返回值
    /// 返回匹配的账号记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id_including_deleted_with_session(
        &self,
        id: &str,
        session: &mut ClientSession,
    ) -> Result<Option<AccountCore>> {
        let account = self
            .database()
            .collection::<AccountCore>(self.collection_name())
            .find_one(doc! { "id": id })
            .session(session)
            .await?;
        Ok(account)
    }
}
