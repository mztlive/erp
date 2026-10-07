//! 导入任务是文件的持久归属；识别失败不等于合同归档。
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::{ContractExtraction, ImportFailure, OcrDocument};
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
    Succeeded,
}

#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct ContractImport {
    #[serde(flatten)]
    pub base: BaseModel,
    pub owner_id: String,
    pub command: ImportCommand,
    pub source: ImportSource,
    pub status: ImportStatus,
    pub started_at: Option<u64>,
    pub ocr: Option<OcrDocument>,
    pub extraction: Option<ContractExtraction>,
    pub failure: Option<ImportFailure>,
    pub result: Option<UploadContractView>,
    #[serde(default)]
    pub customer_id: Option<String>,
}

/// 任务公开视图不含对象存储键和全文，只提供逐字段依据。
#[derive(Clone, Serialize)]
pub struct ImportView {
    pub expected_customer_id: Option<String>,
    pub revision_target: Option<RevisionTarget>,
    pub recoverable_at: Option<u64>,
    pub id: String,
    pub version: u64,
    pub file_name: String,
    pub page_count: u32,
    pub status: ImportStatus,
    pub started_at: Option<u64>,
    pub extraction: Option<ContractExtraction>,
    pub failure: Option<ImportFailure>,
    pub result: Option<UploadContractView>,
    #[serde(default)]
    pub customer_id: Option<String>,
}

impl From<ContractImport> for ImportView {
    fn from(task: ContractImport) -> Self {
        Self {
            expected_customer_id: task.command.expected_customer_id,
            revision_target: task.command.revision_target,
            recoverable_at: task.started_at.map(|start| start.saturating_add(IMPORT_RECOVERY_SECONDS)),
            id: task.base.id,
            version: task.base.version,
            file_name: task.source.file_name,
            page_count: task.source.page_count,
            status: task.status,
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
    /// # 参数
    /// 无。
    /// # 返回
    /// 格式合法时成功。
    /// # 错误
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
    /// # 参数
    /// * `id` / `owner_id` / `command` / `source` - 服务端身份、认证人、控制命令及源文件。
    /// # 返回
    /// 只拥有文件且未归档的任务。
    /// # 错误
    /// 非法控制命令。
    pub fn new(id: String, owner_id: String, command: ImportCommand, source: ImportSource) -> Result<Self> {
        command.validate()?;
        Ok(Self {
            base: BaseModel::new(id),
            owner_id,
            command,
            source,
            status: ImportStatus::Ready,
            started_at: None,
            ocr: None,
            extraction: None,
            failure: None,
            result: None,
            customer_id: None,
        })
    }

    /// 开始或恢复过期处理；仓储版本 CAS 防止并发执行。
    /// # 参数
    /// * `now` - 当前秒级时间；恢复等待覆盖外部识别及归档确认窗口。
    /// # 返回
    /// 已成功返回 false，其余可开始时返回 true。
    /// # 错误
    /// 正在处理且未过期时拒绝。
    pub fn begin(&mut self, now: u64) -> Result<bool> {
        if self.status == ImportStatus::Succeeded {
            return Ok(false);
        }
        if self.status == ImportStatus::Processing
            && self.started_at.is_some_and(|start| now < start.saturating_add(IMPORT_RECOVERY_SECONDS))
        {
            return Err(Error::ConflictError("合同正在识别，请稍后查看结果".into()));
        }
        self.status = ImportStatus::Processing;
        self.started_at = Some(now);
        self.failure = None;
        self.ocr = None;
        self.extraction = None;
        Ok(true)
    }

    /// 同一请求键只允许重放同一份文件及上下文。
    /// # 参数
    /// * `command` / `sha256` - 重试请求及文件摘要。
    /// # 返回
    /// 相同时成功。
    /// # 错误
    /// 异载荷复用时冲突。
    pub fn replay(&self, command: &ImportCommand, sha256: &str) -> Result<()> {
        if self.command != *command || self.source.sha256 != sha256 {
            return Err(Error::ConflictError("导入请求已用于另一份文件或业务上下文，请重新发起导入".into()));
        }
        Ok(())
    }
}
