//! 合同识别导入：文件任务持久化、事务外识别、事务内重验并原子归档。
pub mod aliyun;
mod archive;
mod audit;
mod customer;
mod matching;
pub mod openai;
mod pipeline;
mod progress;
mod source;
#[cfg(test)]
mod tests;
use application_core::AuditActor;
use erp_contract::PageView;
use erp_contract::entity::recognition::{ContractImport, ImportFailure, ImportStatus, ImportView};
use erp_contract::repository::recognition::{self, ContractImportExt};
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::NoTransaction;
pub use pipeline::RecognitionProviders;
use progress::TaskProgress;
use sha2::{Digest, Sha256};
pub use source::inspect_pdf;

use crate::{Error, Result};

/// 创建人拥有任务；归档时另外执行客户与合同 DataScope 重验。
#[derive(Clone)]
pub struct ContractImportProcess {
    db: Database,
    rbac: SharedRbacService,
    providers: RecognitionProviders,
}

/// 已领取的识别工作；内容不可由 HTTP 调用者构造或改写。
pub struct ImportAttempt {
    task: ContractImport,
}

impl ContractImportProcess {
    /// 装配供应商无关流程。
    /// # 参数
    /// * `db` / `rbac` / `providers` - 数据库、权限服务和识别端口。
    /// # 返回
    /// 完整流程实例。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService, providers: RecognitionProviders) -> Self {
        Self { db, rbac, providers }
    }

    /// 查询本人任务。
    /// # 参数
    /// * `id` / `actor` - 任务及认证人。
    /// # 返回
    /// 不含全文和存储键的视图。
    /// # 错误
    /// 不存在、越权或数据库读取失败。
    pub async fn detail(&self, id: &str, actor: &AuditActor) -> Result<ImportView> {
        Ok(recognition::owned(&self.db, actor.id(), id, &mut NoTransaction).await?.into())
    }

    /// 分页查询本人历史，包含失败与已成功任务。
    /// # 参数
    /// * `page` / `revision_contract_id` / `actor` - 页码、可选追加目标及认证人。
    /// # 返回
    /// 每页 20 项。
    /// # 错误
    /// 数据库读取失败。
    pub async fn list(
        &self,
        page: u64,
        revision_contract_id: Option<&str>,
        actor: &AuditActor,
    ) -> Result<PageView<ImportView>> {
        let result =
            recognition::list(&self.db, actor.id(), page, revision_contract_id, &mut NoTransaction).await?;
        Ok(PageView {
            items: result.items.into_iter().map(Into::into).collect(),
            total: result.total,
            page: result.page,
            page_size: result.page_size,
        })
    }

    /// 领取本人任务；后台执行器必须独立持有本调用至识别结果保存。
    /// # 参数
    /// * `id` / `actor` - 任务与认证人。
    /// # 返回
    /// 当前视图与待执行工作；成功重放不产生工作。
    /// # 错误
    /// 无权、源文件不可用、版本竞争或数据库失败。
    pub async fn claim(&self, id: &str, actor: &AuditActor) -> Result<(ImportView, Option<ImportAttempt>)> {
        let mut task = recognition::owned(&self.db, actor.id(), id, &mut NoTransaction).await?;
        if !task.begin(now())? {
            return Ok((task.into(), None));
        }
        self.require_source(&task, &mut NoTransaction).await?;
        self.db.contract_imports().update(&mut task, &mut NoTransaction).await?;
        Ok((task.clone().into(), Some(ImportAttempt { task })))
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
        _actor: &AuditActor,
        pdf: &[u8],
    ) -> Result<ImportView> {
        let mut task = attempt.task;
        if task.source.sha256 != hex::encode(Sha256::digest(pdf)) {
            return self
                .fail(task, ImportFailure::new("SOURCE_CHANGED", "原文件内容已变化，请重新上传"))
                .await;
        }
        let attempt = self
            .providers
            .run(pdf, task.source.page_count, &mut TaskProgress { db: &self.db, task: &mut task })
            .await?;
        task.ocr = attempt.ocr;
        task.extraction = attempt.extraction;
        if let Some(failure) = attempt.failure {
            return self.fail(task, failure).await;
        }
        let ocr = task.ocr.take().ok_or_else(|| Error::Internal("识别结果缺少 OCR".into()))?;
        let extraction =
            task.extraction.take().ok_or_else(|| Error::Internal("识别结果缺少提取信息".into()))?;
        task.finish_recognition(ocr, extraction)?;
        self.db.contract_imports().update(&mut task, &mut NoTransaction).await?;
        Ok(task.into())
    }

    /// 登记读取原文件失败，任务可在原记录重试。
    /// # 参数
    /// * `attempt` - 已领取且尚未执行识别的工作。
    /// # 返回
    /// 持久失败任务。
    /// # 错误
    /// 版本竞争或持久化失败。
    pub async fn source_failed(&self, attempt: ImportAttempt) -> Result<ImportView> {
        self.fail(attempt.task, ImportFailure::new("SOURCE_UNAVAILABLE", "合同文件读取失败，请稍后重试"))
            .await
    }

    async fn fail(&self, mut task: ContractImport, failure: ImportFailure) -> Result<ImportView> {
        task.status = ImportStatus::Failed;
        task.failure = Some(failure);
        self.db.contract_imports().update(&mut task, &mut NoTransaction).await?;
        Ok(task.into())
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}
