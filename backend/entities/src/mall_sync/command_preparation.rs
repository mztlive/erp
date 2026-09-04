//! W17 触发与映射命令的输入整备：文本规范化、版本解析与证据去重。
//!
//! 数据库装载、事务与授权仍由调用方完成；本域只判定已进入服务的
//! typed DTO 是否满足可执行形态，并返回规范化值与强类型失配，
//! 传输错误类别由调用方按失配原因映射。

use super::MallSyncTriggerSource;
use crate::source_registry::MallSyncStage;

/// 命令输入整备失配的强类型原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPreparationError {
    /// 文本去除首尾空白后为空，携带调用方字段标签。
    BlankText(String),
    /// 版本不是十进制正整数字符串，携带调用方字段标签。
    NotPositiveInteger(String),
    /// 版本解析为零，携带调用方字段标签。
    ZeroVersion(String),
    /// 所需证据条目含空白项。
    BlankEvidence,
    /// 所需证据条目在去空白后重复。
    DuplicateEvidence,
    /// 执行阶段不是一期商城主导。
    NotFirstPhase,
    /// 系统定时增量携带人工理由。
    ScheduledWithReason,
}

impl std::fmt::Display for CommandPreparationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlankText(label) => write!(formatter, "{label}不能为空"),
            Self::NotPositiveInteger(label) => {
                write!(formatter, "{label}必须是正整数字符串")
            }
            Self::ZeroVersion(label) => write!(formatter, "{label}必须大于0"),
            Self::BlankEvidence => formatter.write_str("所需证据条目不能为空"),
            Self::DuplicateEvidence => formatter.write_str("所需证据条目不能重复"),
            Self::NotFirstPhase => formatter.write_str("W17 只接受一期商城主导阶段的执行命令"),
            Self::ScheduledWithReason => formatter.write_str("系统定时增量不得携带人工理由"),
        }
    }
}

/// 去除首尾空白并拒绝空文本。
///
/// # 参数
/// * `value` - 调用方传入的原始文本
/// * `label` - 调用方字段标签，用于定位错误字段
///
/// # 返回
/// 返回去空白后的规范化文本。
///
/// # 错误
/// 规范化后为空时返回携带字段标签的 `BlankText`。
///
/// # 约束
/// 纯值转换，不访问数据库；不改变大小写与内部空白。
pub fn prepare_text(
    value: &str,
    label: &'static str,
) -> std::result::Result<String, CommandPreparationError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(CommandPreparationError::BlankText(label.to_string()));
    }
    Ok(normalized.to_string())
}

/// 解析十进制正整数版本字符串。
///
/// # 参数
/// * `value` - 调用方传入的版本字符串
/// * `label` - 调用方字段标签，用于定位错误字段
///
/// # 返回
/// 返回解析后的版本号。
///
/// # 错误
/// 非十进制整数时返回 `NotPositiveInteger`；解析为零时返回 `ZeroVersion`。
///
/// # 约束
/// 纯值转换，不访问数据库；前后空白视为可接受输入并忽略。
pub fn parse_positive_version(
    value: &str,
    label: &'static str,
) -> std::result::Result<u64, CommandPreparationError> {
    let parsed = value
        .trim()
        .parse::<u64>()
        .map_err(|_| CommandPreparationError::NotPositiveInteger(label.to_string()))?;
    if parsed == 0 {
        return Err(CommandPreparationError::ZeroVersion(label.to_string()));
    }
    Ok(parsed)
}

/// 规范化所需证据清单并拒绝空白与重复。
///
/// # 参数
/// * `values` - 调用方传入的证据条目
///
/// # 返回
/// 返回逐项去空白后的规范化清单，顺序保持调用方顺序。
///
/// # 错误
/// 任一条目去空白后为空时返回 `BlankEvidence`；
/// 去空白后存在重复时返回 `DuplicateEvidence`。
///
/// # 约束
/// 纯值转换，不访问数据库；空集合本身合法，长度上下限仍由 DTO 校验持有。
pub fn prepare_evidence_list(values: &[String]) -> std::result::Result<Vec<String>, CommandPreparationError> {
    let normalized = values
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    if normalized.iter().any(|value| value.is_empty()) {
        return Err(CommandPreparationError::BlankEvidence);
    }
    let unique = normalized.iter().collect::<std::collections::HashSet<_>>();
    if unique.len() != normalized.len() {
        return Err(CommandPreparationError::DuplicateEvidence);
    }
    Ok(normalized)
}

/// 校验触发命令的执行阶段为准入阶段。
///
/// # 参数
/// * `stage` - 调用方冻结的执行阶段
///
/// # 返回
/// 一期商城主导时返回 `Ok(())`。
///
/// # 错误
/// 其他阶段返回 `NotFirstPhase`。
///
/// # 约束
/// 纯值规则，不访问数据库；缺失来源与停用判定仍由调用方完成。
pub fn check_trigger_stage(stage: MallSyncStage) -> std::result::Result<(), CommandPreparationError> {
    if stage != MallSyncStage::FirstPhaseMallOwned {
        return Err(CommandPreparationError::NotFirstPhase);
    }
    Ok(())
}

/// 按触发来源解析人工理由与触发人。
///
/// # 参数
/// * `source` - 触发来源
/// * `reason` - 调用方携带的人工理由
/// * `actor_id` - 当前操作人 ID
///
/// # 返回
/// 返回 `(触发理由, 触发人)`；定时触发两者均为 `None`。
///
/// # 错误
/// 定时触发携带理由时返回 `ScheduledWithReason`；
/// 人工触发理由为空时返回携带 `人工触发理由` 标签的 `BlankText`。
///
/// # 约束
/// 纯值规则，不访问数据库；操作人身份的授权判定仍由调用方持有。
pub fn resolve_trigger_actor(
    source: MallSyncTriggerSource,
    reason: Option<&str>,
    actor_id: &str,
) -> std::result::Result<(Option<String>, Option<String>), CommandPreparationError> {
    match source {
        MallSyncTriggerSource::Scheduled if reason.is_none() => Ok((None, None)),
        MallSyncTriggerSource::Scheduled => Err(CommandPreparationError::ScheduledWithReason),
        MallSyncTriggerSource::Manual => Ok((
            Some(prepare_text(reason.unwrap_or_default(), "人工触发理由")?),
            Some(actor_id.to_string()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check_trigger_stage, parse_positive_version, prepare_evidence_list, prepare_text,
        resolve_trigger_actor, CommandPreparationError,
    };
    use crate::mall_sync::MallSyncTriggerSource;
    use crate::source_registry::MallSyncStage;

    #[test]
    fn trigger_text_trims_and_rejects_blank() {
        assert_eq!(prepare_text(" 核对 ", "核对理由").unwrap(), "核对");
        assert_eq!(
            prepare_text("   ", "幂等键"),
            Err(CommandPreparationError::BlankText("幂等键".to_string()))
        );
    }

    #[test]
    fn trigger_stage_only_accepts_first_phase() {
        assert!(check_trigger_stage(MallSyncStage::FirstPhaseMallOwned).is_ok());
        assert_eq!(
            check_trigger_stage(MallSyncStage::Archived),
            Err(CommandPreparationError::NotFirstPhase)
        );
    }

    #[test]
    fn trigger_actor_matrix_covers_scheduled_and_manual() {
        assert_eq!(
            resolve_trigger_actor(MallSyncTriggerSource::Scheduled, None, "scheduler").unwrap(),
            (None, None)
        );
        assert_eq!(
            resolve_trigger_actor(
                MallSyncTriggerSource::Scheduled,
                Some("不应携带理由"),
                "scheduler"
            ),
            Err(CommandPreparationError::ScheduledWithReason)
        );
        assert_eq!(
            resolve_trigger_actor(MallSyncTriggerSource::Manual, Some(" 人工核对 "), "actor-1").unwrap(),
            (Some("人工核对".to_string()), Some("actor-1".to_string()))
        );
        assert!(matches!(
            resolve_trigger_actor(MallSyncTriggerSource::Manual, None, "actor-1"),
            Err(CommandPreparationError::BlankText(_))
        ));
        assert!(matches!(
            resolve_trigger_actor(MallSyncTriggerSource::Manual, Some("   "), "actor-1"),
            Err(CommandPreparationError::BlankText(_))
        ));
    }

    #[test]
    fn positive_version_rejects_non_integer_and_zero() {
        assert_eq!(parse_positive_version(" 2 ", "待办版本").unwrap(), 2);
        assert_eq!(
            parse_positive_version("0", "待办版本"),
            Err(CommandPreparationError::ZeroVersion("待办版本".to_string()))
        );
        for invalid in ["", "   ", "1.5", "-1", "abc", "18446744073709551616"] {
            assert_eq!(
                parse_positive_version(invalid, "待办版本"),
                Err(CommandPreparationError::NotPositiveInteger(
                    "待办版本".to_string()
                )),
                "非法版本必须定位字段并拒绝: {invalid:?}"
            );
        }
    }

    #[test]
    fn evidence_list_normalizes_and_rejects_blank_or_duplicate() {
        assert_eq!(
            prepare_evidence_list(&[" a ".to_string(), "b".to_string()]).unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(
            prepare_evidence_list(&["a".to_string(), "   ".to_string()]),
            Err(CommandPreparationError::BlankEvidence)
        );
        assert_eq!(
            prepare_evidence_list(&["a".to_string(), " a ".to_string()]),
            Err(CommandPreparationError::DuplicateEvidence)
        );
        assert!(prepare_evidence_list(&[]).unwrap().is_empty());
    }
}
