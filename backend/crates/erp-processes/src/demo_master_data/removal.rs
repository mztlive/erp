//! 一次事务清空演示数据库，仅保留 admin 账号及必要授权。

use application_core::AuditActor;
use persistence_core::DatabaseReset;
use serde::Serialize;

use super::DemoMasterDataService;
use crate::{Error, Result};

/// 全库重置结果，不提供继续删除的分批游标。
#[derive(Debug, Serialize)]
pub struct DemoResetReport {
    /// 实际物理删除的数据库文档总数。
    pub deleted_documents: u64,
}

impl DemoMasterDataService {
    /// 清空配置数据库，仅原样保留 admin 和必要超管授权。
    /// # 参数
    /// `actor` 必须是具有内建超管授权的 admin 本人。
    /// # 返回
    /// 单个事务删除的文档总数；集合和索引保留。
    /// # 错误
    /// 功能未开放、身份不符合保留要求、策略并发变化、删除或提交失败时返回错误。
    pub async fn reset_database(&self, actor: &AuditActor) -> Result<DemoResetReport> {
        self.ensure_enabled()?;
        let revision = self.rbac.authorize_database_reset(actor).await?;
        let reset = DatabaseReset::prepare(self.db.clone()).await?;
        let actor = actor.clone();
        let rbac = self.rbac.clone();
        self.rbac
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move {
                    let retained = rbac.database_reset_retention(&actor, executor).await?;
                    let deleted_documents = reset.execute(retained, executor).await?;
                    Ok::<_, Error>(DemoResetReport { deleted_documents })
                })
            })
            .await
    }
}
