//! 一次事务清空演示数据库，仅保留 admin 账号及必要授权。

use application_core::AuditActor;
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::DatabaseReset;
use serde::Serialize;

use super::DemoMasterDataProcess;
use crate::{Error, Result};

/// 全库重置结果，不提供继续删除的分批游标。
#[derive(Debug, Serialize)]
pub struct DemoResetReport {
    /// 实际物理删除的数据库文档总数。
    pub deleted_documents: u64,
}

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    /// 重置已开启演示功能的数据库，仅允许具有内建超管授权的 `admin` 本人执行。
    /// 在同一事务原样保留该账号及必要授权，返回物理删除条数；集合和索引不删除。
    ///
    /// 演示开关关闭、身份或策略版本不符、删除或提交失败时返回错误。
    pub async fn reset_database(&self, actor: &AuditActor) -> Result<DemoResetReport> {
        self.ensure_enabled()?;
        let revision = self.identity.authorize_database_reset(actor).await?;

        let reset = DatabaseReset::prepare(self.db.clone()).await?;
        let actor = actor.clone();
        let identity = self.identity.clone();
        self.identity
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move {
                    let retained = identity.database_reset_retention(&actor, executor).await?;
                    let deleted_documents = reset.execute(retained, executor).await?;
                    Ok::<_, Error>(DemoResetReport { deleted_documents })
                })
            })
            .await
    }
}
