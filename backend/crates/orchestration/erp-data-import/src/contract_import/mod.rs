//! 合同识别导入：文件任务持久化、事务外识别、事务内重验并原子归档。
mod archive;
mod audit;
mod customer;
mod matching;
mod pipeline;
mod progress;
mod read;
mod source;
#[cfg(test)]
mod tests;
use std::sync::Arc;

use application_core::AuditActor;
use erp_contract::entity::recognition::{ContractImport, ImportFailure, ImportStatus, ImportView};
use erp_contract::repository::recognition::{self, ContractImportExt};
use erp_contract::{ContractAccess, ContractService};
use erp_customer::CustomerAccess;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
pub use pipeline::RecognitionProviders;
use progress::TaskProgress;
use sha2::{Digest, Sha256};
pub use source::inspect_pdf;

use crate::{Error, Result};

/// 合同导入实际消费的客户与合同范围能力，由应用装配。
#[derive(Clone)]
pub struct ContractImportServices {
    pub contracts: Arc<ContractService>,
    pub contract_access: ContractAccess,
    pub customer_access: CustomerAccess,
}

/// 创建人拥有任务；归档时另外执行客户与合同 DataScope 重验。
#[derive(Clone)]
pub struct ContractImportProcess {
    db: Database,
    services: ContractImportServices,
    providers: RecognitionProviders,
}

/// 已领取的识别工作；内容不可由 HTTP 调用者构造或改写。
pub struct ImportAttempt {
    task: ContractImport,
    actor: AuditActor,
}

impl ContractImportProcess {
    /// 装配供应商无关流程。
    /// # 参数
    /// * `db` / `services` / `providers` - 数据库、已装配范围能力的领域服务和识别端口。
    /// # 返回
    /// 完整流程实例。
    /// # 错误
    /// 无。
    pub fn new(db: Database, services: ContractImportServices, providers: RecognitionProviders) -> Self {
        Self { db, services, providers }
    }

    /// 领取本人任务；后台执行器必须独立持有本调用至识别结果保存。
    /// # 参数
    /// * `id` / `actor` - 任务与认证人。
    /// # 返回
    /// 当前视图与待执行工作；成功重放不产生工作。
    /// # 错误
    /// 无权、源文件不可用、版本竞争或数据库失败。
    pub async fn claim(&self, id: &str, actor: &AuditActor) -> Result<(ImportView, Option<ImportAttempt>)> {
        let process = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.claim_stage(&id, &actor, executor).await })
            })
            .await
    }

    /// 功能资格、本人任务及版本更新使用同一个领取事务。
    async fn claim_stage(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(ImportView, Option<ImportAttempt>)> {
        self.services.contract_access.require_import(actor, executor).await?;
        let mut task = recognition::owned(&self.db, actor.id(), id, executor).await?;
        if !task.begin(now())? {
            return Ok((task.into(), None));
        }
        self.require_source(&task, executor).await?;
        self.db.contract_imports().update(&mut task, executor).await?;
        Ok((task.clone().into(), Some(ImportAttempt { task, actor: actor.clone() })))
    }

    /// 完成已领取任务；调用方必须在独立后台任务中等待收尾。
    /// # 参数
    /// * `attempt` / `actor` / `pdf` - 已领取工作、认证人及原文件。
    /// # 返回
    /// 成功或带失败原因的持久任务。
    /// # 错误
    /// 版本竞争、持久化失败或提交结果未知。
    pub async fn execute(
        &self,
        attempt: ImportAttempt,
        actor: &AuditActor,
        pdf: &[u8],
    ) -> Result<ImportView> {
        let mut task = attempt.task;
        self.detail(&task.base.id, actor).await?;
        if task.owner_id != actor.id() {
            return Err(Error::Forbidden("合同识别任务不属于当前账号".into()));
        }
        if task.source.sha256 != hex::encode(Sha256::digest(pdf)) {
            return self
                .fail(task, actor, ImportFailure::new("SOURCE_CHANGED", "原文件内容已变化，请重新上传"))
                .await;
        }
        let attempt = self
            .providers
            .run(
                pdf,
                task.source.page_count,
                &mut TaskProgress {
                    db: &self.db,
                    access: &self.services.contract_access,
                    actor,
                    task: &mut task,
                },
            )
            .await?;
        task.ocr = attempt.ocr;
        task.extraction = attempt.extraction;
        if let Some(failure) = attempt.failure {
            return self.fail(task, actor, failure).await;
        }
        let ocr = task.ocr.take().ok_or_else(|| Error::Internal("识别结果缺少 OCR".into()))?;
        let extraction =
            task.extraction.take().ok_or_else(|| Error::Internal("识别结果缺少提取信息".into()))?;
        task.finish_recognition(ocr, extraction)?;
        Ok(save_task(&self.db, &self.services.contract_access, actor, task).await?.into())
    }

    /// 登记读取原文件失败，任务可在原记录重试。
    /// # 参数
    /// * `attempt` - 已领取且尚未执行识别的工作。
    /// # 返回
    /// 持久失败任务。
    /// # 错误
    /// 版本竞争或持久化失败。
    pub async fn source_failed(&self, attempt: ImportAttempt) -> Result<ImportView> {
        self.fail(
            attempt.task,
            &attempt.actor,
            ImportFailure::new("SOURCE_UNAVAILABLE", "合同文件读取失败，请稍后重试"),
        )
        .await
    }

    async fn fail(
        &self,
        mut task: ContractImport,
        actor: &AuditActor,
        failure: ImportFailure,
    ) -> Result<ImportView> {
        task.status = ImportStatus::Failed;
        task.failure = Some(failure);
        Ok(save_task(&self.db, &self.services.contract_access, actor, task).await?.into())
    }
}

/// 事务外识别的进度或结果只在重新授权后提交，外部服务不进入事务。
async fn save_task(
    db: &Database,
    access: &ContractAccess,
    actor: &AuditActor,
    mut task: ContractImport,
) -> Result<ContractImport> {
    let db = db.clone();
    let access = access.clone();
    let actor = actor.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                access.require_import(&actor, executor).await?;
                if task.owner_id != actor.id() {
                    return Err(Error::Forbidden("合同识别任务不属于当前账号".into()));
                }
                db.contract_imports().update(&mut task, executor).await?;
                Ok(task)
            })
        })
        .await
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}
