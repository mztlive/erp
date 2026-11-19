//! 业务服务使用的账号身份、候选目录与显示资料。
use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::AccountKind;
use mongodb::Database;
use persistence_core::Executor;

use crate::repository::prelude::*;
use crate::{AccessControlExt, AccountSummary, Result};
/// 为业务服务提供不含登录凭证的账号摘要和候选人。调用方根据自己的业务场景完成对象和岗位授权。
#[derive(Clone)]
pub struct AccountDirectoryService {
    // 账号目录使用的数据库；查询沿用调用方的执行器。
    db: Database,
}
impl AccountDirectoryService {
    /// 绑定应用数据库，供业务用例按需读取当前账号资料。创建服务时只保存数据库句柄。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 为已知账号 ID 读取未删除账号的摘要，供对象展示或身份核对。账号缺失时返回 `None`，仓储错误原样返回。
    pub async fn account(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<AccountSummary>> {
        Ok(self.db.accounts().find_account(id, executor).await?.as_ref().map(AccountSummary::from))
    }

    /// 批量把已知账号 ID 补充为展示摘要，适合填充业务列表。返回实际存在且未删除的账号，结果可以少于请求项。
    pub async fn accounts(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<AccountSummary>> {
        Ok(self.db.accounts().list_by_ids(ids, executor).await?.iter().map(AccountSummary::from).collect())
    }

    /// 按账号类型读取候选摘要，供选择器或业务预检使用。调用方可在候选集上继续检查岗位或对象资格。
    pub async fn accounts_by_kind(
        &self,
        kind: AccountKind,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountSummary>> {
        Ok(self.db.accounts().list_by_kind(kind, executor).await?.iter().map(AccountSummary::from).collect())
    }

    /// 按姓名或登录名限量搜索有效后台账号，供审批候选人选择等场景使用。调用方继续核验候选人的具体审批资格。
    pub async fn active_backoffice(
        &self,
        search: Option<&str>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountSummary>> {
        Ok(self
            .db
            .accounts()
            .list_active_approval_candidates(search, limit, executor)
            .await?
            .iter()
            .map(AccountSummary::from)
            .collect())
    }

    /// 从当前账号数据复验认证主体的身份类型和可用状态。账号失效或身份不符时返回 `false`；仓储错误原样返回。
    pub async fn actor_is_active(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<bool> {
        Ok(self.account(actor.id(), executor).await?.is_some_and(|account| account.matches_actor(actor)))
    }

    /// 批量补全已授权业务对象引用的人员姓名。使用调用方快照，只返回未删除账号的 ID 和姓名。
    pub async fn names(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(self.db.accounts().names_by_ids(ids, executor).await?)
    }
}
