//! 业务服务使用的账号身份、候选目录与显示资料。
use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::AccountKind;
use mongodb::Database;
use persistence_core::Executor;

use crate::repository::prelude::*;
use crate::{AccessControlExt, AccountSummary, Result};
/// 读取不含凭证的账号摘要；业务对象权限由调用方独立证明。
#[derive(Clone)]
pub struct AccountDirectoryService {
    db: Database,
}
impl AccountDirectoryService {
    /// 绑定账号仓储。
    /// # 参数
    /// `db` 为应用数据库。
    /// # 返回
    /// 返回无缓存的目录服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    /// 按精确身份读取账号摘要。
    /// # 参数
    /// id 为账号 ID，executor 沿用调用方执行器。
    /// # 返回
    /// 未删除账号的摘要；缺失时返回 None。
    /// # 错误
    /// 数据库读取失败透传。
    pub async fn account(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<AccountSummary>> {
        Ok(self.db.accounts().find_account(id, executor).await?.as_ref().map(AccountSummary::from))
    }

    /// 按精确身份批量读取账号摘要。
    /// # 参数
    /// ids 为账号 ID 集合，executor 沿用调用方执行器。
    /// # 返回
    /// 已存在的未删除账号，不补造缺失项。
    /// # 错误
    /// 数据库读取失败透传。
    pub async fn accounts(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<AccountSummary>> {
        Ok(self.db.accounts().list_by_ids(ids, executor).await?.iter().map(AccountSummary::from).collect())
    }

    /// 读取指定类型的账号候选，业务资格由调用方继续核验。
    /// # 参数
    /// kind 为账号类型，executor 为调用方执行器。
    /// # 返回
    /// 对应类型的账号摘要。
    /// # 错误
    /// 数据库读取失败透传。
    pub async fn accounts_by_kind(
        &self,
        kind: AccountKind,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccountSummary>> {
        Ok(self.db.accounts().list_by_kind(kind, executor).await?.iter().map(AccountSummary::from).collect())
    }

    /// 有界搜索有效内部账号；不附带审批业务权限。
    /// # 参数
    /// search 为搜索词，limit 为上限，executor 为调用方执行器。
    /// # 返回
    /// 满足姓名或登录账号搜索的有效内部账号。
    /// # 错误
    /// 数据库读取失败透传。
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

    /// 重验认证主体仍为同类型的可登录账号。
    /// # 参数
    /// actor 为认证身份，executor 为当前执行器。
    /// # 返回
    /// 身份一致且账号有效时返回 true。
    /// # 错误
    /// 基础设施失败不按账号失效处理。
    pub async fn actor_is_active(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<bool> {
        Ok(self.account(actor.id(), executor).await?.is_some_and(|account| account.matches_actor(actor)))
    }

    /// 批量读取调用方已授权对象引用的账号姓名。
    /// # 参数
    /// `ids` 为对象引用的账号 ID，`executor` 沿用调用方快照。
    /// # 返回
    /// 仅返回未删除账号的 ID 与姓名；不存在的账号不补造。
    /// # 错误
    /// 账号仓储读取失败时返回错误。
    pub async fn names(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        Ok(self.db.accounts().names_by_ids(ids, executor).await?)
    }
}
