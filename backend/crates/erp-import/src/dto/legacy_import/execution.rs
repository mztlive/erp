//! 导入执行强类型命令与结果的 DTO。

use application_core::non_blank;
use erp_core::ids::LegacyImportBatchId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::super::receipt::{optional_text, parse_command_version, required_text};
use super::ImportJobStatus;
use crate::entity::legacy_import::LegacyImportBatchStatus;
use crate::error::{Error, Result};

/// 导入应用阶段强命令动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportExecutionAction {
    /// 提交应用：待应用批次进入导入中，后台任务开始运行。
    StartApply,
    /// 取消尚未应用的项；已形成的业务事实不回滚。
    CancelPending,
    /// 仅把上一轮失败项重新准备为待应用。
    RetryFailed,
}

impl ImportExecutionAction {
    /// 返回稳定的 wire code。
    ///
    /// # 返回
    /// 返回 `START_APPLY` / `CANCEL_PENDING` / `RETRY_FAILED`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StartApply => "START_APPLY",
            Self::CancelPending => "CANCEL_PENDING",
            Self::RetryFailed => "RETRY_FAILED",
        }
    }
}

/// W18 导入执行强类型命令。
///
/// 命令与责任确认分离：确认完成只使批次就绪，本命令中的
/// `START_APPLY` 才能启动后台应用。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ImportExecutionCommand {
    /// 命令锁定的导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 客户端最近读取到的批次乐观锁版本（wire string）。
    #[validate(custom(function = "non_blank", message = "批次版本不能为空"))]
    pub expected_batch_version: String,
    /// 命令锁定的当前试算版本；提交应用和重试失败项时必填。
    pub expected_trial_version: Option<String>,
    /// 执行动作。
    pub action: ImportExecutionAction,
    /// 结构化原因码；取消尚未应用项时必填。
    #[validate(length(max = 128, message = "原因码过长"))]
    pub reason_code: Option<String>,
    /// 操作说明。
    #[validate(length(max = 1024, message = "操作说明过长"))]
    pub comment: Option<String>,
    /// 请求幂等身份；原值不进入审计消息。
    #[validate(
        length(max = 128, message = "请求身份过长"),
        custom(function = "non_blank", message = "请求身份不能为空")
    )]
    pub request_id: String,
}

/// 导入执行命令的稳定结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportExecutionResultStatus {
    /// 后台应用已启动。
    Started,
    /// 尚未应用的项已取消。
    Cancelled,
    /// 失败项已重新准备，尚未启动应用。
    RetryPrepared,
    /// 交易结果待核实；当前同步实现不主动返回此值。
    Unknown,
}

/// 导入执行命令完成后的固定下一步。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportExecutionNextStep {
    /// 查看后台应用进度。
    MonitorProgress,
    /// 查看取消后的最终分区结果。
    ReviewResult,
    /// 失败项已准备，需要显式提交应用。
    StartApply,
}

/// W18 导入执行强命令稳定结果信封。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportExecutionResult {
    /// 本次执行动作。
    pub action: ImportExecutionAction,
    /// 命令结果状态。
    pub result_status: ImportExecutionResultStatus,
    /// 导入批次 ID。
    pub batch_id: String,
    /// 交易提交后的批次状态。
    pub batch_status: LegacyImportBatchStatus,
    /// 交易提交后的批次版本（wire string）。
    pub batch_version: String,
    /// 本次命令锁定的试算版本。
    pub trial_version: Option<String>,
    /// 对应后台任务 ID。
    pub background_job_id: String,
    /// 交易提交后的后台任务状态。
    pub background_job_status: ImportJobStatus,
    /// 交易提交后的后台任务版本（wire string）。
    pub background_job_version: String,
    /// 本次启动、取消或重新准备的项数。
    pub affected_items: u64,
    /// 服务端确定的下一步。
    pub next_step: ImportExecutionNextStep,
    /// 不含原始 `request_id` 的稳定审计收据 ID。
    pub audit_receipt: String,
}

/// 已规范化的导入执行命令（INT-E24 DTO 归属）。
///
/// 持有强类型版本与动作专属试算/原因矩阵已校验的字段；Service 只解释
/// 持久化事实，不再做二次解析。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImportExecution {
    /// 命令锁定的导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 客户端读取到的批次乐观锁版本。
    pub expected_batch_version: u64,
    /// 命令锁定的试算版本；提交应用和重试失败项时必填。
    pub expected_trial_version: Option<u32>,
    /// 执行动作。
    pub action: ImportExecutionAction,
    /// 结构化原因码；取消时必填，提交应用时禁止。
    pub reason_code: Option<String>,
    /// 操作说明。
    pub comment: Option<String>,
    /// 请求幂等身份。
    pub request_id: String,
}

impl TryFrom<ImportExecutionCommand> for PreparedImportExecution {
    type Error = Error;

    /// 从强类型命令构造已规范化的执行准备（INT-E24 纯转换）。
    ///
    /// # 参数
    /// * `command` - 强类型执行命令
    ///
    /// # 返回
    /// 返回动作矩阵已校验的准备。
    ///
    /// # 错误
    /// 版本非法或动作专属试算/原因矩阵非法时返回错误。
    fn try_from(command: ImportExecutionCommand) -> Result<Self> {
        let expected_trial_version = command
            .expected_trial_version
            .as_deref()
            .map(|value| parse_command_version(value, "试算版本"))
            .transpose()?;
        if matches!(command.action, ImportExecutionAction::StartApply | ImportExecutionAction::RetryFailed)
            && expected_trial_version.is_none()
        {
            return Err(Error::ValidationError("提交应用或重试失败项必须携带试算版本".to_string()));
        }
        let reason_code = optional_text(command.reason_code);
        if command.action == ImportExecutionAction::CancelPending && reason_code.is_none() {
            return Err(Error::ValidationError("取消尚未应用项必须提供原因码".to_string()));
        }
        if command.action == ImportExecutionAction::StartApply && reason_code.is_some() {
            return Err(Error::ValidationError("提交应用不得携带取消或重试原因码".to_string()));
        }
        Ok(Self {
            batch_id: command.batch_id,
            expected_batch_version: parse_command_version(&command.expected_batch_version, "批次版本")?,
            expected_trial_version,
            action: command.action,
            reason_code,
            comment: optional_text(command.comment),
            request_id: required_text(&command.request_id, "请求身份不能为空")?,
        })
    }
}
