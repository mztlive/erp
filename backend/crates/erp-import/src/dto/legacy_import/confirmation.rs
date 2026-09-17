//! 导入业务确认的 DTO（确认创建、强类型完成命令与确认列表查询）。

use application_core::{non_blank, normalized_text};
use erp_core::ids::{LegacyImportBatchId, WorkItemId};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::super::receipt::{optional_text, parse_command_version, required_text};
use super::{LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS, PageParams, normalize_paging};
use crate::entity::legacy_import::{ConfirmationDecision, ConfirmationScope, ConfirmationStatus};
use crate::error::{Error, Result};

/// 导入确认创建请求（数据模型 §6.12：每批按版本化确认矩阵为必要范围各创建一个事实）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateLegacyImportConfirmationRequest {
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 责任范围（销售、采购、运营、仓储、财务等）。
    #[validate(custom(function = "non_blank", message = "确认范围不能为空"))]
    pub confirmation_scope: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本（`(batch_id, scope, trial_version)` 唯一）。
    pub trial_version: u32,
    /// 本次确认针对的导入规则版本。
    #[validate(custom(function = "non_blank", message = "导入规则版本不能为空"))]
    pub import_rule_version: String,
}

/// 导入业务确认的领域决定。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ImportBusinessConfirmationDecision {
    /// 命令锁定的导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 客户端最近查询到的批次乐观锁版本。
    #[validate(custom(function = "non_blank", message = "批次版本不能为空"))]
    pub expected_batch_version: String,
    /// 命令锁定的试算版本。
    #[validate(custom(function = "non_blank", message = "试算版本不能为空"))]
    pub expected_trial_version: String,
    /// 服务端固定注册表中的责任范围。
    #[validate(custom(function = "non_blank", message = "确认范围不能为空"))]
    pub confirmation_scope: String,
    /// 确认本范围或退回修复。
    pub action: ConfirmationDecision,
    /// 退回原因代码（退回时必填）。
    pub reason_code: Option<String>,
    /// 意见说明（确认意见可选）。
    pub comment: Option<String>,
}

/// `CompleteImportBusinessConfirmation` 强类型命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CompleteImportBusinessConfirmationCommand {
    /// 当前 `IMPORT_BUSINESS_CONFIRMATION` 正式任务。
    pub work_item_id: WorkItemId,
    /// 客户端读取到的任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    pub expected_task_version: String,
    /// 任务冻结的批次试算主体版本。
    #[validate(custom(function = "non_blank", message = "任务主体版本不能为空"))]
    pub expected_subject_version: String,
    /// 业务对象、版本与正式结论只存在于该强类型信封。
    #[validate(nested)]
    pub decision: ImportBusinessConfirmationDecision,
    /// 正式操作幂等键；只以服务端不可逆摘要进入审计。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 强类型确认命令的最终结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportBusinessConfirmationResultStatus {
    /// 已确认责任范围。
    Confirmed,
    /// 已形成退回修复结论。
    Rejected,
    /// 结果未知；当前同步实现不主动返回此值。
    Unknown,
}

/// 强类型确认完成后的固定下一步。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportBusinessConfirmationNextStep {
    /// 等待同一试算矩阵的其它责任范围。
    AwaitOtherConfirmations,
    /// 全部必要范围已确认，可进入应用阶段。
    StartApply,
    /// 修复问题并产生新试算版本。
    FixAndRevalidate,
}

/// 已规范化的确认完成命令（INT-E24 DTO 归属）。
///
/// 持有强类型版本与动作专属必填/禁止矩阵已校验的字段；Service 只解释
/// 持久化事实，不再做二次解析或字符串中转。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedConfirmationCompletion {
    /// 当前正式任务。
    pub work_item_id: WorkItemId,
    /// 命令锁定的导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 客户端读取到的任务乐观锁版本。
    pub expected_task_version: u64,
    /// 任务冻结的批次试算主体版本。
    pub expected_subject_version: String,
    /// 客户端读取到的批次乐观锁版本。
    pub expected_batch_version: u64,
    /// 命令锁定的试算版本。
    pub expected_trial_version: u32,
    /// 已注册确认范围。
    pub confirmation_scope: String,
    /// 确认或退回动作。
    pub decision: ConfirmationDecision,
    /// 退回原因代码（退回时必填，确认时禁止）。
    pub reason_code: Option<String>,
    /// 意见说明。
    pub comment: Option<String>,
    /// 正式操作幂等键。
    pub idempotency_key: String,
}

impl TryFrom<CompleteImportBusinessConfirmationCommand> for PreparedConfirmationCompletion {
    type Error = Error;

    /// 从强类型命令构造已规范化的完成准备（INT-E24 纯转换）。
    ///
    /// # 参数
    /// * `command` - 强类型完成命令
    ///
    /// # 返回
    /// 返回动作矩阵已校验的准备。
    ///
    /// # 错误
    /// 范围未注册、版本非法或动作专属原因矩阵非法时返回错误。
    fn try_from(command: CompleteImportBusinessConfirmationCommand) -> Result<Self> {
        let decision = command.decision;
        let confirmation_scope = ConfirmationScope::parse(&decision.confirmation_scope)?.as_str().to_string();
        let reason_code = optional_text(decision.reason_code);
        if decision.action == ConfirmationDecision::ReturnForFix && reason_code.is_none() {
            return Err(Error::ValidationError("退回修复必须提供原因代码".to_string()));
        }
        if decision.action == ConfirmationDecision::ConfirmScope && reason_code.is_some() {
            return Err(Error::ValidationError("确认责任范围不得携带退回原因".to_string()));
        }
        Ok(Self {
            work_item_id: command.work_item_id,
            batch_id: decision.batch_id,
            expected_task_version: parse_command_version(&command.expected_task_version, "任务版本")?,
            expected_subject_version: required_text(
                &command.expected_subject_version,
                "任务主体版本不能为空",
            )?,
            expected_batch_version: parse_command_version(&decision.expected_batch_version, "批次版本")?,
            expected_trial_version: parse_command_version(&decision.expected_trial_version, "试算版本")?,
            confirmation_scope,
            decision: decision.action,
            reason_code,
            comment: optional_text(decision.comment),
            idempotency_key: required_text(&command.idempotency_key, "幂等键不能为空")?,
        })
    }
}

/// 导入确认列表查询参数（按批次查询为主）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LegacyImportConfirmationListParams {
    /// 所属导入批次。
    pub batch_id: Option<LegacyImportBatchId>,
    /// 责任范围筛选。
    pub confirmation_scope: Option<String>,
    /// 确认状态筛选。
    pub status: Option<ConfirmationStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`trial_version`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的导入确认列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImportConfirmationListQuery {
    /// 所属导入批次。
    pub batch_id: Option<LegacyImportBatchId>,
    /// 责任范围筛选。
    pub confirmation_scope: Option<String>,
    /// 确认状态筛选。
    pub status: Option<ConfirmationStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl LegacyImportConfirmationListParams {
    /// 归一化导入确认列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub fn normalized(&self) -> Result<LegacyImportConfirmationListQuery> {
        Ok(LegacyImportConfirmationListQuery {
            batch_id: self.batch_id.clone(),
            confirmation_scope: normalized_text(self.confirmation_scope.as_deref()),
            status: self.status,
            paging: normalize_paging(
                &self.sort_by,
                &self.sort_dir,
                self.page,
                self.page_size,
                LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS,
            )?,
        })
    }
}
