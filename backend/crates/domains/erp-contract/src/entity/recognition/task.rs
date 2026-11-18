//! 导入任务是文件的持久归属；识别失败不等于合同归档。
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::{ConfirmImport, ContractExtraction, ImportFailure, OcrDocument, RecognitionDraft};
use crate::{Error, Result, UploadContractView};

/// 中断任务恢复等待覆盖文件读取、识别与事务确认窗口。
pub const IMPORT_RECOVERY_SECONDS: u64 = 600;

/// HTTP 只接受控制信息，未知业务字段一律拒绝。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportCommand {
    pub request_key: String,
    pub expected_customer_id: Option<String>,
    pub revision_target: Option<RevisionTarget>,
}

/// 追加修订同样重新识别，不接受业务字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionTarget {
    pub contract_id: String,
    pub version: u64,
}

/// 文件信息仅由服务端生成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSource {
    pub file_asset_id: String,
    pub file_name: String,
    pub sha256: String,
    pub page_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Ready,
    Processing,
    Failed,
    Review,
    Succeeded,
}

/// 当前执行阶段，仅由后台在开始实际工作前推进。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStage {
    ReadingFile,
    Ocr,
    AiExtract,
    PreparingReview,
}

#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct ContractImport {
    #[serde(flatten)]
    pub base: BaseModel,
    pub owner_id: String,
    pub command: ImportCommand,
    pub source: ImportSource,
    pub status: ImportStatus,
    #[serde(default)]
    pub stage: Option<ImportStage>,
    pub started_at: Option<u64>,
    pub ocr: Option<OcrDocument>,
    pub extraction: Option<ContractExtraction>,
    pub failure: Option<ImportFailure>,
    pub result: Option<UploadContractView>,
    #[serde(default)]
    pub confirmation: Option<ConfirmImport>,
    #[serde(default)]
    pub customer_id: Option<String>,
}

/// 任务公开视图提供可编辑预填、逐字段依据及结果，不含存储键和全文。
#[derive(Clone, Serialize)]
pub struct ImportView {
    /// 本人上传的源文件；用于销售开单材料，写入仍须校验资产归属与可用性。
    pub source_file_asset_id: String,
    pub draft: Option<RecognitionDraft>,
    pub expected_customer_id: Option<String>,
    pub revision_target: Option<RevisionTarget>,
    pub recoverable_at: Option<u64>,
    pub id: String,
    pub version: u64,
    pub file_name: String,
    pub page_count: u32,
    pub status: ImportStatus,
    pub stage: Option<ImportStage>,
    pub started_at: Option<u64>,
    pub extraction: Option<ContractExtraction>,
    pub failure: Option<ImportFailure>,
    pub result: Option<UploadContractView>,
    #[serde(default)]
    pub customer_id: Option<String>,
}

impl From<ContractImport> for ImportView {
    fn from(task: ContractImport) -> Self {
        let draft = task
            .extraction
            .as_ref()
            .zip(task.ocr.as_ref())
            .map(|(extraction, document)| extraction.draft(document));
        Self {
            source_file_asset_id: task.source.file_asset_id,
            draft,
            expected_customer_id: task.command.expected_customer_id,
            revision_target: task.command.revision_target,
            recoverable_at: task.started_at.map(|start| start.saturating_add(IMPORT_RECOVERY_SECONDS)),
            id: task.base.id,
            version: task.base.version,
            file_name: task.source.file_name,
            page_count: task.source.page_count,
            status: task.status,
            stage: task.stage,
            started_at: task.started_at,
            extraction: task.extraction,
            failure: task.failure,
            result: task.result,
            customer_id: task.customer_id,
        }
    }
}

impl ImportCommand {
    /// 校验稳定请求键和可选上下文，禁止空身份。
    /// 格式合法时成功。
    /// 请求键或目标非法。
    pub fn validate(&self) -> Result<()> {
        if self.request_key.len() < 8
            || self.request_key.len() > 100
            || !self.request_key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || self.expected_customer_id.as_ref().is_some_and(|id| id.trim().is_empty() || id.len() > 100)
            || self.revision_target.as_ref().is_some_and(|t| {
                t.contract_id.trim().is_empty() || t.contract_id.len() > 100 || t.version == 0
            })
        {
            return Err(Error::ValidationError("导入请求无效，请刷新后重试".into()));
        }
        Ok(())
    }
}

impl ContractImport {
    /// 创建待识别任务，初始状态不能用于合同或销售开单。
    /// 只拥有文件且未归档的任务。
    /// `id` / `owner_id` / `command` / `source` - 服务端身份、认证人、控制命令及源文件。
    /// 非法控制命令。
    pub fn new(id: String, owner_id: String, command: ImportCommand, source: ImportSource) -> Result<Self> {
        command.validate()?;
        Ok(Self {
            base: BaseModel::new(id),
            owner_id,
            command,
            source,
            status: ImportStatus::Ready,
            stage: None,
            started_at: None,
            ocr: None,
            extraction: None,
            failure: None,
            result: None,
            confirmation: None,
            customer_id: None,
        })
    }

    /// 开始或恢复过期处理；仓储版本 CAS 防止并发执行。
    /// 已完成识别或归档返回 false，其余可开始时返回 true。
    /// `now` 为当前秒级时间；恢复等待覆盖外部识别及归档确认窗口。
    /// 正在处理且未过期时拒绝。
    pub fn begin(&mut self, now: u64) -> Result<bool> {
        if matches!(self.status, ImportStatus::Succeeded | ImportStatus::Review) {
            return Ok(false);
        }
        if self.status == ImportStatus::Processing
            && self.started_at.is_some_and(|start| now < start.saturating_add(IMPORT_RECOVERY_SECONDS))
        {
            return Err(Error::ConflictError("合同正在识别，请稍后查看结果".into()));
        }

        self.status = ImportStatus::Processing;
        self.stage = Some(ImportStage::ReadingFile);
        self.started_at = Some(now);
        self.failure = None;
        self.ocr = None;
        self.extraction = None;
        Ok(true)
    }

    /// 推进本轮处理阶段，禁止跳过、倒退或修改已结束任务。
    /// 阶段合法时更新任务。
    /// `stage` 为即将执行的阶段。
    /// 任务状态或阶段顺序不合法。
    pub fn advance(&mut self, stage: ImportStage) -> Result<()> {
        let next = matches!(
            (self.stage, stage),
            (Some(ImportStage::ReadingFile), ImportStage::Ocr)
                | (Some(ImportStage::Ocr), ImportStage::AiExtract)
                | (Some(ImportStage::AiExtract), ImportStage::PreparingReview)
        );
        if self.status != ImportStatus::Processing || !next {
            return Err(Error::ConflictError("识别阶段已变化，请刷新查看结果".into()));
        }
        self.stage = Some(stage);
        Ok(())
    }

    /// OCR 与字段提取结束后，将全文和部分提取结果保存到识别中的任务并进入待确认态，不创建归档合同。非识别中状态返回冲突。
    pub fn finish_recognition(&mut self, ocr: OcrDocument, extraction: ContractExtraction) -> Result<()> {
        if self.status != ImportStatus::Processing {
            return Err(Error::ConflictError("任务未在识别中，请刷新查看结果".into()));
        }

        self.ocr = Some(ocr);
        self.extraction = Some(extraction);
        self.failure = None;
        self.status = ImportStatus::Review;
        Ok(())
    }

    /// 用户确认识别结果时，只有待确认且版本匹配才返回 `true` 进入归档。
    /// 已成功归档的任务只接受完全相同的确认命令并返回 `false`，避免重复归档；未就绪、版本变化或异载荷均返回冲突。
    pub fn check_confirmation(&self, command: &ConfirmImport) -> Result<bool> {
        if self.status == ImportStatus::Succeeded {
            return if self.confirmation.as_ref() == Some(command) {
                Ok(false)
            } else {
                Err(Error::ConflictError("此任务已归档，确认内容不一致，请刷新查看结果".into()))
            };
        }

        if self.status != ImportStatus::Review || self.base.version != command.version {
            return Err(Error::ConflictError("识别结果或版本已变化，请刷新后确认".into()));
        }
        Ok(true)
    }

    /// 同请求键重试识别导入时，要求原命令上下文和源文件 SHA-256 完全一致；任一不同则冲突，不把新文件或上下文并入原任务。
    pub fn replay(&self, command: &ImportCommand, sha256: &str) -> Result<()> {
        if self.command != *command || self.source.sha256 != sha256 {
            return Err(Error::ConflictError("导入请求已用于另一份文件或业务上下文，请重新发起导入".into()));
        }
        Ok(())
    }
}
