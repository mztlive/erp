//! 文件与导入任务原子登记，失败任务继续拥有原文件。
use application_core::AuditActor;
use erp_contract::entity::recognition::{
    ContractImport, ImportCommand, ImportSource, ImportView, pdf_page_count,
};
use erp_contract::repository::recognition::{self, ContractImportExt};
use erp_core::ids::FileAssetId;
use erp_support::{FileAsset, FileAssetExt, RegisterFileAssetRequest, SecurityScanStatus};
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use sha2::{Digest, Sha256};
use validator::Validate;

use super::ContractImportProcess;
use super::audit::{CreateImport, context};
use crate::adapters::contract_access;
use crate::audit::run_audited_event;
use crate::{Error, Result};

/// 独立解析 PDF 页数和原文件摘要，不能相信 OCR 报告的页数。
/// # 参数
/// * `pdf` - 已校验上传大小的 PDF 字节。
/// # 返回
/// SHA256 和页数。
/// # 错误
/// 损坏、加密、空文件或超过 200 页。
pub async fn inspect_pdf(pdf: Vec<u8>) -> Result<(String, u32)> {
    tokio::task::spawn_blocking(move || {
        let pages = pdf_page_count(&pdf).map_err(|e| Error::ValidationError(e.message))?;
        Ok((hex::encode(Sha256::digest(&pdf)), pages))
    })
    .await
    .map_err(|_| Error::Internal("PDF 检查失败".into()))?
}

impl ContractImportProcess {
    /// 登记源文件及任务；重放时返回 false，调用方删除本次未使用对象。
    /// # 参数
    /// * `command` / `asset_request` / `digest` / `actor` - 控制信息、受控文件、摘要页数与认证人。
    /// # 返回
    /// 任务及是否接纳本次对象。
    /// # 错误
    /// 异载荷重放、授权失败、事务失败或未知提交；未知提交禁止清理对象。
    pub async fn create(
        &self,
        command: ImportCommand,
        asset_request: RegisterFileAssetRequest,
        digest: (String, u32),
        actor: &AuditActor,
    ) -> Result<(ImportView, bool)> {
        command.validate()?;
        asset_request.validate()?;
        let asset = FileAsset::new(FileAssetId::new(next_id()), asset_request.into_data(actor.id())?)?;
        run_audited_event(
            &self.db,
            context(actor, "contract.import.create", "contract_import")?,
            CreateImport { process: self.clone(), actor: actor.clone(), command, asset, digest },
        )
        .await
    }

    /// 在当前执行器内登记源文件和导入任务；同人同请求键重放时不写新记录。
    ///
    /// # 参数
    /// * `command` - 导入控制信息。
    /// * `asset` - 已构造的源文件资产。
    /// * `digest` - 原文件 SHA256 与页数。
    /// * `actor` - 认证人。
    /// * `executor` - 创建事务执行器。
    ///
    /// # 返回
    /// 任务视图，以及本次对象是否被接纳。重放命中时第二项为 `false`。
    ///
    /// # 错误
    /// 重放载荷不一致、追加目标无权更新、任务构造失败，或文件与任务仓储写入失败时返回对应错误。
    pub(super) async fn create_task(
        &self,
        command: ImportCommand,
        asset: FileAsset,
        digest: (String, u32),
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(ImportView, bool)> {
        if let Some(original) =
            recognition::replay(&self.db, actor.id(), &command.request_key, executor).await?
        {
            original.replay(&command, &digest.0)?;
            return Ok((original.into(), false));
        }
        if let Some(target) = &command.revision_target {
            contract_access(self.db.clone(), self.rbac.clone())
                .require_with(actor.clone(), "update", &target.contract_id, executor)
                .await?;
        }
        let task = ContractImport::new(
            next_id(),
            actor.id().into(),
            command,
            ImportSource {
                file_asset_id: asset.base.id.clone(),
                file_name: asset.file_name.clone(),
                sha256: digest.0,
                page_count: digest.1,
            },
        )?;
        self.db.file_assets().create(&asset, executor).await?;
        self.db.contract_imports().create(&task, executor).await?;
        Ok((task.into(), true))
    }

    /// 获取本人任务的可用源文件；文件 ID 不独立授权。
    /// # 参数
    /// * `id` / `actor` - 任务与认证人。
    /// # 返回
    /// 仅供受控读取的源文件。
    /// # 错误
    /// 无权、文件销毁、隔离或缺失。
    pub async fn source(&self, id: &str, actor: &AuditActor) -> Result<FileAsset> {
        let task = recognition::owned(&self.db, actor.id(), id, &mut NoTransaction).await?;
        self.require_source(&task, &mut NoTransaction).await
    }

    /// 读取任务绑定的源文件，并拒绝已销毁、拒绝或隔离的文件。
    ///
    /// # 参数
    /// * `task` - 已归属的导入任务。
    /// * `executor` - 读取所用执行器。
    ///
    /// # 返回
    /// 可继续读取的源文件。
    ///
    /// # 错误
    /// 文件不存在时返回 `NotFound`；已销毁、扫描拒绝或隔离时返回 `BusinessLogicError`；仓储读取失败时返回对应错误。
    pub(super) async fn require_source(
        &self,
        task: &ContractImport,
        executor: &mut dyn Executor,
    ) -> Result<FileAsset> {
        let file = self
            .db
            .file_assets()
            .find_by_id(&task.source.file_asset_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合同源文件不可用".into()))?;
        if file.destroyed_at.is_some()
            || matches!(
                file.security_scan_status,
                SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined
            )
        {
            return Err(Error::BusinessLogicError("合同源文件不可用，请重新上传".into()));
        }
        Ok(file)
    }
}
