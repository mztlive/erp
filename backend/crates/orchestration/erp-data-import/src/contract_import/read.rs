//! 合同识别任务的资格、本人过滤和结果读取共用一个快照。
use application_core::AuditActor;
use erp_contract::PageView;
use erp_contract::entity::recognition::{ContractImport, ImportView};
use erp_contract::repository::recognition;
use erp_support::FileAsset;
use persistence_core::{Executor, Transactional};

use super::ContractImportProcess;
use crate::Result;

impl ContractImportProcess {
    /// 查询本人任务并在同一快照核验合同创建资格。
    /// # 参数
    /// id 为任务，actor 为当前账号。
    /// # 返回
    /// 不含全文和存储键的任务视图。
    /// # 错误
    /// 账号无效、资格不足、任务不归属当前人或读取失败时拒绝。
    pub async fn detail(&self, id: &str, actor: &AuditActor) -> Result<ImportView> {
        Ok(self.task(id, actor).await?.into())
    }

    /// 在独立读取快照中取得本人任务，后续写入仍须在原写事务重验。
    pub(super) async fn task(&self, id: &str, actor: &AuditActor) -> Result<ContractImport> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.task_stage(&id, &actor, executor).await })
            })
            .await
    }

    /// 在原快照完成资格和任务归属读取。
    pub(super) async fn task_stage(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ContractImport> {
        self.services.contract_access.require_import(actor, executor).await?;
        Ok(recognition::owned(&self.db, actor.id(), id, executor).await?)
    }

    /// 查询本人识别历史，功能资格、分页和计数使用同一快照。
    /// # 参数
    /// page 为页码，revision_contract_id 为可选追加目标，actor 为当前账号。
    /// # 返回
    /// 每页二十条的任务视图。
    /// # 错误
    /// 账号或资格无效、事务或读取失败时拒绝。
    pub async fn list(
        &self,
        page: u64,
        revision_contract_id: Option<&str>,
        actor: &AuditActor,
    ) -> Result<PageView<ImportView>> {
        let process = self.clone();
        let target = revision_contract_id.map(str::to_owned);
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.services.contract_access.require_import(&actor, executor).await?;
                    let result =
                        recognition::list(&process.db, actor.id(), page, target.as_deref(), executor).await?;
                    Ok(PageView {
                        items: result.items.into_iter().map(Into::into).collect(),
                        total: result.total,
                        page: result.page,
                        page_size: result.page_size,
                    })
                })
            })
            .await
    }

    /// 获取本人任务的可用原文件，授权与读取使用原事务。
    /// # 参数
    /// id 为任务身份，actor 为当前账号。
    /// # 返回
    /// 仅供受控读取的文件资产。
    /// # 错误
    /// 无资格、非本人任务、文件不可用或事务失败时拒绝。
    pub async fn source(&self, id: &str, actor: &AuditActor) -> Result<FileAsset> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let task = process.task_stage(&id, &actor, executor).await?;
                    process.require_source(&task, executor).await
                })
            })
            .await
    }
}
