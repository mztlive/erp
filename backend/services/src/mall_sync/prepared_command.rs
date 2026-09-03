//! W17 触发与映射命令的整备 DTO：经 `TryFrom` 生成可执行形态。
//!
//! 线上形态、字段名与错误定位保持历史契约；规范化与矩阵规则委托
//! `entities::mall_sync::command_preparation`，本层只做错误类别映射与
//! 命令重组，不重新实现纯规则。

use entities::mall_sync::command_preparation::{self, CommandPreparationError};

use super::dto::TriggerMallSyncCommand;
use super::dto::{ConfirmMappingCommand, ReapplyMallSnapshotCommand, RequestSourceFixCommand};
use crate::errors::{Error, Result};

/// 将领域整备失配映射为稳定的传输错误。
///
/// # 参数
/// * `error` - 领域返回的强类型整备失配
///
/// # 返回
/// 非一期阶段返回业务失败关闭；其余返回字段级校验失败。
///
/// # 错误
/// 本函数本身不失败；返回的错误由调用方直接抛出。
///
/// # 约束
/// 纯错误映射，不访问数据库；文案与领域 `Display` 保持一致。
pub(crate) fn preparation_error(error: CommandPreparationError) -> Error {
    match &error {
        CommandPreparationError::NotFirstPhase => Error::BusinessLogicError(error.to_string()),
        CommandPreparationError::BlankText(_)
        | CommandPreparationError::NotPositiveInteger(_)
        | CommandPreparationError::ZeroVersion(_)
        | CommandPreparationError::BlankEvidence
        | CommandPreparationError::DuplicateEvidence
        | CommandPreparationError::ScheduledWithReason => Error::ValidationError(error.to_string()),
    }
}

/// 规范化后的触发命令，可直接用于作业规格推导与幂等身份。
///
/// # 用途
/// 持有去空白后的幂等键与各模式规范化字段。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 仅经 `TryFrom` 构造；不得手工拼装未经阶段与模式校验的实例。
#[derive(Debug, Clone)]
pub(crate) struct PreparedTriggerCommand {
    /// 规范化后的完整触发命令。
    command: TriggerMallSyncCommand,
    /// 去空白后的幂等键。
    idempotency_key: String,
}

impl PreparedTriggerCommand {
    /// 返回规范化后的完整触发命令。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回可直接匹配的规范化触发命令引用。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库；调用方不得再做二次规范化。
    pub(crate) fn command(&self) -> &TriggerMallSyncCommand {
        &self.command
    }

    /// 返回去空白后的幂等键。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回规范化幂等键字符串切片。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库。
    pub(crate) fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }
}

impl TryFrom<&TriggerMallSyncCommand> for PreparedTriggerCommand {
    type Error = Error;

    /// 从线上触发命令生成规范化整备形态。
    ///
    /// # 参数
    /// * `command` - 调用方传入的原始触发命令
    ///
    /// # 返回
    /// 返回阶段、幂等键与各模式字段均已规范化的整备命令。
    ///
    /// # 错误
    /// 幂等键空白、非一期阶段、定时/人工理由矩阵非法，
    /// 或单号/理由文本空白时返回对应传输错误。
    ///
    /// # 约束
    /// 纯值转换，不访问数据库；触发人与水位版本判定仍由调用方完成。
    fn try_from(command: &TriggerMallSyncCommand) -> Result<Self> {
        let idempotency_key = command_preparation::prepare_text(command.idempotency_key(), "幂等键")
            .map_err(preparation_error)?;
        command_preparation::check_trigger_stage(command.execution_stage()).map_err(preparation_error)?;
        let mut normalized = command.clone();
        match &mut normalized {
            TriggerMallSyncCommand::Incremental {
                trigger_source,
                reason,
                idempotency_key: key,
                ..
            } => {
                let source = *trigger_source;
                let prepared = match source {
                    entities::mall_sync::MallSyncTriggerSource::Scheduled if reason.is_none() => None,
                    entities::mall_sync::MallSyncTriggerSource::Scheduled => {
                        return Err(preparation_error(CommandPreparationError::ScheduledWithReason));
                    }
                    entities::mall_sync::MallSyncTriggerSource::Manual => Some(
                        command_preparation::prepare_text(
                            reason.as_deref().unwrap_or_default(),
                            "人工触发理由",
                        )
                        .map_err(preparation_error)?,
                    ),
                };
                *reason = prepared;
                *key = idempotency_key.clone();
            }
            TriggerMallSyncCommand::SingleOrder {
                trigger_source,
                external_order_no,
                reason,
                idempotency_key: key,
                ..
            } => {
                if *trigger_source != entities::mall_sync::MallSyncTriggerSource::Manual {
                    return Err(Error::ValidationError(
                        "按单号补拉只能由授权用户人工触发".to_string(),
                    ));
                }
                *external_order_no = command_preparation::prepare_text(external_order_no, "原来源销售单号")
                    .map_err(preparation_error)?;
                *reason =
                    command_preparation::prepare_text(reason, "人工触发理由").map_err(preparation_error)?;
                *key = idempotency_key.clone();
            }
            TriggerMallSyncCommand::RetryFailedJob {
                reason,
                idempotency_key: key,
                ..
            } => {
                *reason = command_preparation::prepare_text(reason, "重试理由").map_err(preparation_error)?;
                *key = idempotency_key.clone();
            }
            TriggerMallSyncCommand::Reconciliation {
                reason,
                idempotency_key: key,
                ..
            } => {
                *reason = command_preparation::prepare_text(reason, "核对理由").map_err(preparation_error)?;
                *key = idempotency_key.clone();
            }
        }
        Ok(Self {
            command: normalized,
            idempotency_key,
        })
    }
}

/// 规范化后的确认映射命令，持有解析后的待办版本。
///
/// # 用途
/// 持有规范化确认命令与解析后的待办版本号。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 仅经 `TryFrom` 构造；版本二次解析仍被禁止。
#[derive(Debug, Clone)]
pub(crate) struct PreparedConfirmMapping {
    /// 规范化后的确认命令。
    command: ConfirmMappingCommand,
    /// 解析后的待办版本。
    expected_task_version: u64,
}

impl PreparedConfirmMapping {
    /// 返回规范化后的确认命令。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回规范化确认命令引用。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库。
    pub(crate) fn command(&self) -> &ConfirmMappingCommand {
        &self.command
    }

    /// 返回解析后的待办版本。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回调用方冻结的待办版本号。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库；调用方不得再做二次解析。
    pub(crate) fn expected_task_version(&self) -> u64 {
        self.expected_task_version
    }

    /// 返回去空白后的幂等键。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回规范化幂等键字符串切片。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库；调用方必须使用本键构造幂等身份。
    pub(crate) fn idempotency_key(&self) -> &str {
        &self.command.idempotency_key
    }
}

impl TryFrom<&ConfirmMappingCommand> for PreparedConfirmMapping {
    type Error = Error;

    /// 从线上确认命令生成整备形态。
    ///
    /// # 参数
    /// * `command` - 调用方传入的原始确认命令
    ///
    /// # 返回
    /// 返回携带解析后待办版本与规范化幂等键的整备命令。
    ///
    /// # 错误
    /// 待办版本不是正整数字符串，或幂等键空白时返回字段级校验失败。
    ///
    /// # 约束
    /// 纯值转换，不访问数据库；路径一致性仍由调用方校验。
    fn try_from(command: &ConfirmMappingCommand) -> Result<Self> {
        let expected_task_version =
            command_preparation::parse_positive_version(&command.expected_task_version, "待办版本")
                .map_err(preparation_error)?;
        let idempotency_key = command_preparation::prepare_text(&command.idempotency_key, "幂等键")
            .map_err(preparation_error)?;
        let mut normalized = command.clone();
        normalized.idempotency_key = idempotency_key;
        Ok(Self {
            command: normalized,
            expected_task_version,
        })
    }
}

/// 规范化后的来源修复命令，持有版本与证据清单。
///
/// # 用途
/// 持有规范化修复命令、解析后的待办版本与去空白证据清单。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 仅经 `TryFrom` 构造；证据长度上下限仍由 DTO 校验持有。
#[derive(Debug, Clone)]
pub(crate) struct PreparedSourceFix {
    /// 规范化后的修复命令。
    command: RequestSourceFixCommand,
    /// 解析后的待办版本。
    expected_task_version: u64,
    /// 去空白后的证据清单。
    evidence: Vec<String>,
}

impl PreparedSourceFix {
    /// 返回规范化后的修复命令。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回规范化修复命令引用。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库。
    pub(crate) fn command(&self) -> &RequestSourceFixCommand {
        &self.command
    }

    /// 返回解析后的待办版本。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回调用方冻结的待办版本号。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库；调用方不得再做二次解析。
    pub(crate) fn expected_task_version(&self) -> u64 {
        self.expected_task_version
    }

    /// 返回去空白后的证据清单。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回与命令顺序一致的规范化证据清单。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库。
    pub(crate) fn evidence(&self) -> &[String] {
        &self.evidence
    }

    /// 返回去空白后的幂等键。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回规范化幂等键字符串切片。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库；调用方必须使用本键构造幂等身份。
    pub(crate) fn idempotency_key(&self) -> &str {
        &self.command.idempotency_key
    }
}

impl TryFrom<&RequestSourceFixCommand> for PreparedSourceFix {
    type Error = Error;

    /// 从线上修复命令生成整备形态。
    ///
    /// # 参数
    /// * `command` - 调用方传入的原始修复命令
    ///
    /// # 返回
    /// 返回携带解析版本、规范化证据与规范化幂等键的整备命令。
    ///
    /// # 错误
    /// 待办版本非法、幂等键空白，或证据含空白/重复时返回字段级校验失败。
    ///
    /// # 约束
    /// 纯值转换，不访问数据库；路径一致性仍由调用方校验。
    fn try_from(command: &RequestSourceFixCommand) -> Result<Self> {
        let evidence = command_preparation::prepare_evidence_list(&command.action.requested_evidence)
            .map_err(preparation_error)?;
        let expected_task_version =
            command_preparation::parse_positive_version(&command.expected_task_version, "待办版本")
                .map_err(preparation_error)?;
        let idempotency_key = command_preparation::prepare_text(&command.idempotency_key, "幂等键")
            .map_err(preparation_error)?;
        let mut normalized = command.clone();
        normalized.action.requested_evidence = evidence.clone();
        normalized.idempotency_key = idempotency_key;
        Ok(Self {
            command: normalized,
            expected_task_version,
            evidence,
        })
    }
}

/// 规范化后的重新归集命令，持有操作与幂等键。
///
/// # 用途
/// 持有规范化后的重新归集操作 ID 与幂等键。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 仅经 `TryFrom` 构造；原始命令的其余字段保持原样。
#[derive(Debug, Clone)]
pub(crate) struct PreparedReapply {
    /// 规范化后的重新归集命令。
    command: ReapplyMallSnapshotCommand,
    /// 去空白后的操作 ID。
    operation_id: String,
    /// 去空白后的幂等键。
    idempotency_key: String,
}

impl PreparedReapply {
    /// 返回规范化后的重新归集命令。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回规范化命令引用。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库。
    pub(crate) fn command(&self) -> &ReapplyMallSnapshotCommand {
        &self.command
    }

    /// 返回去空白后的操作 ID。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回规范化操作 ID 切片。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库。
    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    /// 返回去空白后的幂等键。
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 返回规范化幂等键切片。
    ///
    /// # 错误
    /// 本方法不失败。
    ///
    /// # 约束
    /// 纯访问器，不访问数据库。
    pub(crate) fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }
}

impl TryFrom<&ReapplyMallSnapshotCommand> for PreparedReapply {
    type Error = Error;

    /// 从线上重新归集命令生成整备形态。
    ///
    /// # 参数
    /// * `command` - 调用方传入的原始重新归集命令
    ///
    /// # 返回
    /// 返回操作 ID 与幂等键均已规范化的整备命令。
    ///
    /// # 错误
    /// 任一字段空白时返回字段级校验失败。
    ///
    /// # 约束
    /// 纯值转换，不访问数据库；路径一致性仍由调用方校验。
    fn try_from(command: &ReapplyMallSnapshotCommand) -> Result<Self> {
        let operation_id = command_preparation::prepare_text(&command.operation_id, "重新归集操作ID")
            .map_err(preparation_error)?;
        let idempotency_key = command_preparation::prepare_text(&command.idempotency_key, "幂等键")
            .map_err(preparation_error)?;
        let mut normalized = command.clone();
        normalized.operation_id = operation_id.clone();
        normalized.idempotency_key = idempotency_key.clone();
        Ok(Self {
            command: normalized,
            operation_id,
            idempotency_key,
        })
    }
}

/// 规范化失败作业 ID 文本。
///
/// # 参数
/// * `value` - 调用方传入的失败作业 ID
///
/// # 返回
/// 返回去空白后的失败作业 ID。
///
/// # 错误
/// 空白时返回字段级校验失败。
///
/// # 约束
/// 纯值转换，不访问数据库；存在性仍由调用方查询判定。
pub(crate) fn prepare_failed_job_id(value: &str) -> Result<String> {
    command_preparation::prepare_text(value, "失败作业 ID").map_err(preparation_error)
}

#[cfg(test)]
mod tests {
    use super::{PreparedConfirmMapping, PreparedReapply, PreparedSourceFix, PreparedTriggerCommand};
    use crate::mall_sync::dto::{
        ConfirmMappingCommand, ReapplyMallSnapshotCommand, RequestSourceFixCommand, TriggerMallSyncCommand,
    };
    use entities::ids::SourceSystemId;
    use entities::mall_sync::MallSyncTriggerSource;
    use entities::source_registry::MallSyncStage;

    /// 构造增量触发命令。
    ///
    /// # 参数
    /// * `source` - 触发来源
    /// * `reason` - 人工理由
    /// * `stage` - 执行阶段
    /// * `idempotency_key` - 幂等键
    ///
    /// # 返回
    /// 返回待整备的触发命令。
    fn incremental_command(
        source: MallSyncTriggerSource,
        reason: Option<&str>,
        stage: MallSyncStage,
        idempotency_key: &str,
    ) -> TriggerMallSyncCommand {
        TriggerMallSyncCommand::Incremental {
            source_system_id: SourceSystemId::new("mall-1"),
            execution_stage: stage,
            trigger_source: source,
            reason: reason.map(str::to_string),
            base_cursor_version: None,
            idempotency_key: idempotency_key.to_string(),
        }
    }

    #[test]
    fn trigger_preparation_covers_scheduled_manual_and_archived() {
        let scheduled = incremental_command(
            MallSyncTriggerSource::Scheduled,
            None,
            MallSyncStage::FirstPhaseMallOwned,
            " request-1 ",
        );
        let prepared = PreparedTriggerCommand::try_from(&scheduled).unwrap();
        assert_eq!(prepared.idempotency_key(), "request-1");

        let scheduled_with_reason = incremental_command(
            MallSyncTriggerSource::Scheduled,
            Some("不应携带理由"),
            MallSyncStage::FirstPhaseMallOwned,
            "request-1",
        );
        assert!(PreparedTriggerCommand::try_from(&scheduled_with_reason).is_err());

        let manual = incremental_command(
            MallSyncTriggerSource::Manual,
            Some(" 人工核对 "),
            MallSyncStage::FirstPhaseMallOwned,
            "request-1",
        );
        let prepared = PreparedTriggerCommand::try_from(&manual).unwrap();
        match prepared.command() {
            TriggerMallSyncCommand::Incremental { reason, .. } => {
                assert_eq!(reason.as_deref(), Some("人工核对"));
            }
            other => panic!("增量命令形态被改变: {other:?}"),
        }

        let archived = incremental_command(
            MallSyncTriggerSource::Manual,
            Some("人工核对"),
            MallSyncStage::Archived,
            "request-1",
        );
        assert!(PreparedTriggerCommand::try_from(&archived).is_err());

        let blank_identity = incremental_command(
            MallSyncTriggerSource::Manual,
            Some("人工核对"),
            MallSyncStage::FirstPhaseMallOwned,
            "   ",
        );
        assert!(PreparedTriggerCommand::try_from(&blank_identity).is_err());
    }

    #[test]
    fn single_order_requires_manual_and_trims_identity() {
        let scheduled_single = TriggerMallSyncCommand::SingleOrder {
            source_system_id: SourceSystemId::new("mall-1"),
            execution_stage: MallSyncStage::FirstPhaseMallOwned,
            trigger_source: MallSyncTriggerSource::Scheduled,
            external_order_no: "MALL-001".to_string(),
            reason: "核对".to_string(),
            idempotency_key: "request-1".to_string(),
        };
        assert!(PreparedTriggerCommand::try_from(&scheduled_single).is_err());

        let manual_single = TriggerMallSyncCommand::SingleOrder {
            source_system_id: SourceSystemId::new("mall-1"),
            execution_stage: MallSyncStage::FirstPhaseMallOwned,
            trigger_source: MallSyncTriggerSource::Manual,
            external_order_no: " MALL-001 ".to_string(),
            reason: " 核对差异后补拉 ".to_string(),
            idempotency_key: "request-1".to_string(),
        };
        let prepared = PreparedTriggerCommand::try_from(&manual_single).unwrap();
        match prepared.command() {
            TriggerMallSyncCommand::SingleOrder {
                external_order_no,
                reason,
                ..
            } => {
                assert_eq!(external_order_no, "MALL-001");
                assert_eq!(reason, "核对差异后补拉");
            }
            other => panic!("按单命令形态被改变: {other:?}"),
        }
    }

    #[test]
    fn mapping_preparation_rejects_bad_version_and_evidence() {
        let confirm: ConfirmMappingCommand = serde_json::from_value(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": " 2 ",
            "expected_subject_version": "1",
            "decision": {
                "mapping_task_id": "mt-1",
                "source_snapshot_id": "snap-1",
                "expected_mapping_task_version": 1,
                "mapping_operation_id": "op-1",
                "execution_stage": "FIRST_PHASE_MALL_OWNED",
                "resolution": {
                    "type": "CONFIRM_TARGET",
                    "object_type": "CUSTOMER",
                    "object_id": "customer-1",
                    "relation_role": "PRIMARY"
                },
                "evidence_note": "已核对客户主体"
            },
            "idempotency_key": "request-1"
        }))
        .unwrap();
        assert_eq!(
            PreparedConfirmMapping::try_from(&confirm)
                .unwrap()
                .expected_task_version(),
            2
        );
        let mut bad_version = confirm.clone();
        bad_version.expected_task_version = "0".to_string();
        assert!(PreparedConfirmMapping::try_from(&bad_version).is_err());
        bad_version.expected_task_version = "1.5".to_string();
        assert!(PreparedConfirmMapping::try_from(&bad_version).is_err());

        let fix: RequestSourceFixCommand = serde_json::from_value(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "2",
            "expected_subject_version": "1",
            "action": {
                "type": "REQUEST_SOURCE_FIX",
                "mapping_task_id": "mt-1",
                "source_snapshot_id": "snap-1",
                "expected_mapping_task_version": 1,
                "request_operation_id": "op-1",
                "reason_code": "SOURCE_MISSING",
                "reason_text": "来源缺失",
                "requested_evidence": ["营业执照"]
            },
            "idempotency_key": "request-1"
        }))
        .unwrap();
        assert_eq!(
            PreparedSourceFix::try_from(&fix).unwrap().evidence(),
            &["营业执照".to_string()]
        );
        let mut duplicate = fix.clone();
        duplicate.action.requested_evidence = vec!["a".to_string(), " a ".to_string()];
        assert!(PreparedSourceFix::try_from(&duplicate).is_err());
        let mut blank = fix.clone();
        blank.action.requested_evidence = vec!["a".to_string(), "   ".to_string()];
        assert!(PreparedSourceFix::try_from(&blank).is_err());

        let reapply: ReapplyMallSnapshotCommand = serde_json::from_value(serde_json::json!({
            "mapping_task_id": "mt-1",
            "source_snapshot_id": "snap-1",
            "expected_mapping_version": 1,
            "operation_id": " op-1 ",
            "execution_stage": "FIRST_PHASE_MALL_OWNED",
            "idempotency_key": " request-1 "
        }))
        .unwrap();
        let prepared = PreparedReapply::try_from(&reapply).unwrap();
        assert_eq!(prepared.operation_id(), "op-1");
        assert_eq!(prepared.idempotency_key(), "request-1");
        let mut blank_operation = reapply.clone();
        blank_operation.operation_id = "   ".to_string();
        assert!(PreparedReapply::try_from(&blank_operation).is_err());
    }

    #[test]
    fn whitespace_variants_share_trigger_replay_identity() {
        use entities::mall_sync::MallSyncCommandIdentity;

        let plain = incremental_command(
            MallSyncTriggerSource::Manual,
            Some("人工核对"),
            MallSyncStage::FirstPhaseMallOwned,
            "request-1",
        );
        let padded = incremental_command(
            MallSyncTriggerSource::Manual,
            Some(" 人工核对 "),
            MallSyncStage::FirstPhaseMallOwned,
            " request-1 ",
        );
        let prepared_plain = PreparedTriggerCommand::try_from(&plain).unwrap();
        let prepared_padded = PreparedTriggerCommand::try_from(&padded).unwrap();
        let payload_plain = serde_json::to_vec(prepared_plain.command()).unwrap();
        let payload_padded = serde_json::to_vec(prepared_padded.command()).unwrap();
        assert_eq!(payload_plain, payload_padded, "空白变体必须序列化为同一指纹载荷");
        let identity_plain = MallSyncCommandIdentity::new(
            "w17-command-",
            "actor-1",
            "trigger-sync",
            "mall-1",
            prepared_plain.idempotency_key(),
            &payload_plain,
        );
        let identity_padded = MallSyncCommandIdentity::new(
            "w17-command-",
            "actor-1",
            "trigger-sync",
            "mall-1",
            prepared_padded.idempotency_key(),
            &payload_padded,
        );
        assert_eq!(identity_plain.audit_id(), identity_padded.audit_id());
        assert_eq!(identity_plain.fingerprint(), identity_padded.fingerprint());
    }

    #[test]
    fn whitespace_variants_share_mapping_replay_identity() {
        use entities::mall_sync::MallSyncCommandIdentity;

        let confirm: ConfirmMappingCommand = serde_json::from_value(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "2",
            "expected_subject_version": "1",
            "decision": {
                "mapping_task_id": "mt-1",
                "source_snapshot_id": "snap-1",
                "expected_mapping_task_version": 1,
                "mapping_operation_id": "op-1",
                "execution_stage": "FIRST_PHASE_MALL_OWNED",
                "resolution": {
                    "type": "CONFIRM_TARGET",
                    "object_type": "CUSTOMER",
                    "object_id": "customer-1",
                    "relation_role": "PRIMARY"
                },
                "evidence_note": "已核对客户主体"
            },
            "idempotency_key": "request-1"
        }))
        .unwrap();
        let mut confirm_padded = confirm.clone();
        confirm_padded.idempotency_key = " request-1 ".to_string();
        let prepared_plain = PreparedConfirmMapping::try_from(&confirm).unwrap();
        let prepared_padded = PreparedConfirmMapping::try_from(&confirm_padded).unwrap();
        assert_eq!(prepared_plain.idempotency_key(), "request-1");
        assert_eq!(prepared_padded.idempotency_key(), "request-1");
        let payload_plain = serde_json::to_vec(prepared_plain.command()).unwrap();
        let payload_padded = serde_json::to_vec(prepared_padded.command()).unwrap();
        assert_eq!(
            payload_plain, payload_padded,
            "确认命令空白幂等键必须回放为同一指纹"
        );
        let identity_plain = MallSyncCommandIdentity::new(
            "w17-command-",
            "actor-1",
            "confirm",
            "mt-1",
            prepared_plain.idempotency_key(),
            &payload_plain,
        );
        let identity_padded = MallSyncCommandIdentity::new(
            "w17-command-",
            "actor-1",
            "confirm",
            "mt-1",
            prepared_padded.idempotency_key(),
            &payload_padded,
        );
        assert_eq!(identity_plain.audit_id(), identity_padded.audit_id());
        assert_eq!(identity_plain.fingerprint(), identity_padded.fingerprint());

        let fix: RequestSourceFixCommand = serde_json::from_value(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "2",
            "expected_subject_version": "1",
            "action": {
                "type": "REQUEST_SOURCE_FIX",
                "mapping_task_id": "mt-1",
                "source_snapshot_id": "snap-1",
                "expected_mapping_task_version": 1,
                "request_operation_id": "op-1",
                "reason_code": "SOURCE_MISSING",
                "reason_text": "来源缺失",
                "requested_evidence": ["a", "b"]
            },
            "idempotency_key": "request-1"
        }))
        .unwrap();
        let mut fix_padded = fix.clone();
        fix_padded.idempotency_key = " request-1 ".to_string();
        fix_padded.action.requested_evidence = vec![" a ".to_string(), "b ".to_string()];
        let prepared_plain = PreparedSourceFix::try_from(&fix).unwrap();
        let prepared_padded = PreparedSourceFix::try_from(&fix_padded).unwrap();
        assert_eq!(prepared_padded.evidence(), &["a".to_string(), "b".to_string()]);
        assert_eq!(
            prepared_padded.command().action.requested_evidence,
            prepared_padded.evidence(),
            "规范化证据必须写回持久化命令"
        );
        let payload_plain = serde_json::to_vec(prepared_plain.command()).unwrap();
        let payload_padded = serde_json::to_vec(prepared_padded.command()).unwrap();
        assert_eq!(
            payload_plain, payload_padded,
            "修复命令空白证据必须回放为同一指纹"
        );
        let identity_plain = MallSyncCommandIdentity::new(
            "w17-command-",
            "actor-1",
            "request-source-fix",
            "mt-1",
            prepared_plain.idempotency_key(),
            &payload_plain,
        );
        let identity_padded = MallSyncCommandIdentity::new(
            "w17-command-",
            "actor-1",
            "request-source-fix",
            "mt-1",
            prepared_padded.idempotency_key(),
            &payload_padded,
        );
        assert_eq!(identity_plain.audit_id(), identity_padded.audit_id());
        assert_eq!(identity_plain.fingerprint(), identity_padded.fingerprint());
    }
}
