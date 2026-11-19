//! AccountCore实体的特化方法

use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::AccountKind;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::AccountCore;

/// 为统一账号集合提供身份查找、候选检索和展示事实读取。
#[allow(async_fn_in_trait)]
pub trait AccountCoreRepositoryExt {
    /// 用调用方执行器按稳定 ID 取未删除账号；无记录返回 `None`，数据库错误直接返回。
    async fn find_account(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<AccountCore>>;

    /// 用调用方执行器按登录账号查找未删除账号；不存在或已软删除返回 `None`，查询错误直接返回。
    async fn find_by_account(
        &self,
        account: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>>;

    /// 检查登录账号占用时按全局唯一索引读取，包含已软删除账号；无记录返回 `None`，数据库错误直接返回。
    async fn find_by_account_including_deleted(
        &self,
        account: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>>;

    /// 批量读取指定 ID 的未删除账号；空输入直接返回空集合，查询错误直接返回。
    async fn list_by_ids(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<AccountCore>>;

    /// 批量取得未删除账号的 ID 到展示名映射；空输入返回空映射，数据库读取或解码错误直接返回。
    async fn names_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;

    /// 为采购责任规则和解析结果加载负责人事实。保留历史入口并委托 [`Self::list_by_ids`]：只返回未删除账号，空输入不查库，查询错误直接返回。
    async fn list_procurement_responsibility_owners(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountCore>>;

    /// 为采购责任展示按稳定 ID 读取负责人账号。保留历史入口并委托 [`Self::find_account`]：未找到未删除账号时返回 `None`，查询错误直接返回。
    async fn find_procurement_responsibility_owner(
        &self,
        owner_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>>;

    /// 按账号类型读取未删除账号，删除条件在 MongoDB 执行，调用方不再内存过滤；查询错误直接返回。
    async fn list_by_kind(&self, kind: AccountKind, executor: &mut dyn Executor) -> Result<Vec<AccountCore>>;

    /// 按审批责任人 ID 读取未删除账号，缺失返回 `None`，查询错误直接返回；账号状态及权限仍由调用方重新核验。
    async fn find_approval_assignee_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>>;

    /// 定义审批规则时检索可选账号，只接受未删除且为 `active + admin` 的记录，调用方不得放宽。
    /// 可在姓名和登录账号中做忽略大小写的字面量搜索；最多返回 `limit` 条，零条上限不查库，查询或反序列化失败直接返回。
    async fn list_active_approval_candidates(
        &self,
        search: Option<&str>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountCore>>;

    /// 为工作项授权按稳定 ID 读取未删除账号；保留历史入口并委托 [`Self::find_account`]，缺失返回 `None`、查询错误直接返回。账号状态及权限由调用方重验。
    async fn find_work_item_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>>;

    /// 为工作项展示批量读取负责人和提交人的账号事实；保留历史入口并委托 [`Self::list_by_ids`]，仅返回未删除账号，空输入不查库，查询错误直接返回。展示名由调用方映射。
    async fn list_work_item_party_accounts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountCore>>;
}

impl AccountCoreRepositoryExt for Repository<'_, AccountCore> {
    // 复用通用仓储按稳定 ID 查询，继承软删除过滤。
    async fn find_account(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<AccountCore>> {
        self.find_by_id(id, executor).await
    }

    // 按 `account` 字段精确定位未删除账号。
    async fn find_by_account(
        &self,
        account: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        self.find_one_by_field("account", account, executor).await
    }

    // 直接查询原集合以跳过软删除过滤，匹配全局登录名唯一约束。
    async fn find_by_account_including_deleted(
        &self,
        account: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        mongo_ops::find_one(&self.collection(), doc! { "account": account }, executor).await
    }

    // 空 ID 集合不发出请求；其余用 `$in` 批量读取未删除账号。
    async fn list_by_ids(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<AccountCore>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    // 将批量账号转换成供展示查找的 ID 到姓名映射。
    async fn names_by_ids(
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

    // 采购负责人批量读取复用统一账号查询，保持相同的删除过滤。
    async fn list_procurement_responsibility_owners(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountCore>> {
        self.list_by_ids(owner_ids, executor).await
    }

    // 单个采购负责人沿用稳定 ID 查找，不另定义账号筛选口径。
    async fn find_procurement_responsibility_owner(
        &self,
        owner_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        self.find_account(owner_id, executor).await
    }

    // 将账号类型和未删除条件同时下推到 MongoDB。
    async fn list_by_kind(&self, kind: AccountKind, executor: &mut dyn Executor) -> Result<Vec<AccountCore>> {
        let filter = doc! {
            "kind": kind.as_str(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        self.find_many(filter, executor).await
    }

    // 按审批责任人的持久化 ID 查账号，不将查到记录视为通过权限校验。
    async fn find_approval_assignee_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        self.find_by_id(id, executor).await
    }

    // 固定后台有效账号条件，转义搜索词后匹配姓名或登录名，并在数据库限制条数。
    async fn list_active_approval_candidates(
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

    // 工作项账号事实复用统一身份查找，不在仓储推断办理资格。
    async fn find_work_item_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<AccountCore>> {
        self.find_account(id, executor).await
    }

    // 复用批量账号查询返回工作项参与人事实，不做显示名投影。
    async fn list_work_item_party_accounts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountCore>> {
        self.list_by_ids(ids, executor).await
    }
}
