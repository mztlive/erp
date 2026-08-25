//! AccountCore实体的特化方法

use std::collections::HashMap;

use super::Repository;
use crate::errors::Result;
use crate::{mongo_ops, Executor};
use entities::{AccountCore, AccountKind};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

impl<'a> Repository<'a, AccountCore> {
    /// 按账号 ID 查找未删除统一账号。
    ///
    /// # 参数
    /// * `id` - 账号 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回匹配的未删除账号；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_account(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<AccountCore>> {
        self.find_by_id(id, executor).await
    }

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

    /// 按账号 ID 集合批量查询统一账号。
    ///
    /// # 参数
    /// * `ids` - 账号 ID 集合；为空时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回全部匹配且未软删除的账号记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn list_by_ids(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<AccountCore>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 按账号 ID 集合批量读取展示名称。
    ///
    /// # 参数
    /// * `ids` - 账号 ID 集合；为空时直接返回空映射
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回以账号 ID 为键、展示名称为值的映射。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn names_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(self
            .list_by_ids(ids, executor)
            .await?
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect())
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

    /// 按审批责任人身份读取账号。
    ///
    /// # 参数
    /// * `id` - 审批责任人账号 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回匹配且未软删除的账号；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本方法只封装审批责任人的持久化身份查询，账号状态与权限仍由调用方重验。
    pub async fn find_approval_assignee_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        self.find_by_id(id, executor).await
    }

    /// 查询定义期可选的有效后台审批账号。
    ///
    /// # 参数
    /// * `search` - 可选姓名或登录账号包含检索，按大小写不敏感字面量匹配
    /// * `limit` - 最大返回条数；为零时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回值
    /// 返回有效后台账号，最多 `limit` 条。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 查询固定限制为 `active + admin`，调用方不得自行拼装更宽的候选范围。
    pub async fn list_active_approval_candidates(
        &self,
        search: Option<&str>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountCore>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut filter = doc! {
            "status": "active",
            "kind": AccountKind::Admin.as_str(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        if let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) {
            let literal = regex::escape(search);
            filter.insert(
                "$or",
                [
                    doc! { "name": { "$regex": &literal, "$options": "i" } },
                    doc! { "account": { "$regex": &literal, "$options": "i" } },
                ],
            );
        }
        let options = FindOptions::builder().limit(i64::from(limit)).build();
        mongo_ops::find_many(&self.collection(), filter, options, executor).await
    }
}
