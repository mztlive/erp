//! W29 受控关闭命令的纯决策与证据引用。

use std::fmt;

use erp_core::{Error, Result};

use super::ResolutionAction;
use super::reconciliation_difference_resolution::RESOLUTION_NO_OVERFLOW_MESSAGE;

/// W29 关闭证据的强类型引用。
///
/// 证据引用领域命令回执，不以展示审计作为关闭依据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct W29EvidenceReference {
    work_item_id: String,
    replacement_work_item_id: Option<String>,
    command_receipt_id: String,
}

impl W29EvidenceReference {
    /// 解析历史持久化格式。
    ///
    /// # 参数
    /// * `value` - `work_item:{id};command_receipt:{id}`，或在两者之间插入
    ///   `replacement_work_item:{id}` 的三段文本。
    ///
    /// # 返回
    /// 返回强类型关闭证据；两段格式没有替代工作项。
    ///
    /// # 错误
    /// 段数不是两段或三段、前缀不匹配、引用为空或 ID 含分号时返回领域错误。
    pub fn parse(value: &str) -> Result<Self> {
        let fields = value.trim().split(';').collect::<Vec<_>>();
        match fields.as_slice() {
            [work_item, audit] => Ok(Self {
                work_item_id: reference_value(work_item, "work_item:")?,
                replacement_work_item_id: None,
                command_receipt_id: reference_value(audit, "command_receipt:")?,
            }),
            [work_item, replacement, audit] => Ok(Self {
                work_item_id: reference_value(work_item, "work_item:")?,
                replacement_work_item_id: Some(reference_value(replacement, "replacement_work_item:")?),
                command_receipt_id: reference_value(audit, "command_receipt:")?,
            }),
            _ => Err(Error::from("领域关闭证据引用格式非法")),
        }
    }

    /// 返回证据对应的关闭动作。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 带替代工作项时返回 `CloseDuplicate`，否则返回 `CloseMisrouted`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn resolution_action(&self) -> ResolutionAction {
        if self.replacement_work_item_id.is_some() {
            ResolutionAction::CloseDuplicate
        } else {
            ResolutionAction::CloseMisrouted
        }
    }

    /// 返回当前工作项 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已解析的当前工作项 ID。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
    }

    /// 返回替代工作项 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 关闭重复任务时返回替代工作项 ID；两段历史格式或误派关闭时返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn replacement_work_item_id(&self) -> Option<&str> {
        self.replacement_work_item_id.as_deref()
    }

    /// 返回命令回执 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已解析的命令回执 ID。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn command_receipt_id(&self) -> &str {
        &self.command_receipt_id
    }
}

impl fmt::Display for W29EvidenceReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.replacement_work_item_id {
            Some(replacement) => write!(
                formatter,
                "work_item:{};replacement_work_item:{};command_receipt:{}",
                self.work_item_id, replacement, self.command_receipt_id
            ),
            None => write!(
                formatter,
                "work_item:{};command_receipt:{}",
                self.work_item_id, self.command_receipt_id
            ),
        }
    }
}

/// W29 关闭命令的规范化纯决策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct W29CloseDecision {
    resolution_action: ResolutionAction,
    replacement_work_item_id: Option<String>,
    close_reason: String,
}

impl W29CloseDecision {
    /// 按稳定原因代码构造关闭决策。
    ///
    /// # 参数
    /// * `reason_code` - 只接受去空白后的 `DUPLICATE` 或 `MISROUTED`。
    /// * `comment` - 可选说明；空白视为缺失。`MISROUTED` 必填。
    /// * `replacement_work_item_id` - 替代工作项。`DUPLICATE` 必填，`MISROUTED` 不得提供。
    ///
    /// # 返回
    /// 返回动作、关闭原因文本和已校验替代工作项都已固定的决策。
    ///
    /// # 错误
    /// 原因代码不是上述二者、重复关闭缺少替代任务、误派关闭带了替代任务或缺少说明、
    /// 或引用为空、含分号时返回领域错误。
    pub fn new(
        reason_code: &str,
        comment: Option<&str>,
        replacement_work_item_id: Option<&str>,
    ) -> Result<Self> {
        let comment = comment.map(str::trim).filter(|value| !value.is_empty());
        match reason_code.trim() {
            "DUPLICATE" => {
                let replacement = required_reference(replacement_work_item_id, "DUPLICATE 必须提供替代任务")?;
                let close_reason = comment.map_or_else(
                    || format!("DUPLICATE replacement={replacement}"),
                    |comment| format!("DUPLICATE replacement={replacement}: {comment}"),
                );
                Ok(Self {
                    resolution_action: ResolutionAction::CloseDuplicate,
                    replacement_work_item_id: Some(replacement),
                    close_reason,
                })
            },
            "MISROUTED" => {
                if replacement_work_item_id.is_some() {
                    return Err(Error::from("MISROUTED 不得提供替代任务"));
                }
                let comment = comment.ok_or_else(|| Error::from("MISROUTED 必须填写原因说明"))?;
                Ok(Self {
                    resolution_action: ResolutionAction::CloseMisrouted,
                    replacement_work_item_id: None,
                    close_reason: format!("MISROUTED: {comment}"),
                })
            },
            _ => Err(Error::from("关闭原因只允许 DUPLICATE 或 MISROUTED")),
        }
    }

    /// 返回领域决定动作。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回构造时固定的 `CloseDuplicate` 或 `CloseMisrouted`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn resolution_action(&self) -> ResolutionAction {
        self.resolution_action
    }

    /// 返回规范化的工作项关闭原因。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回写入工作项的原因文本，含原因代码；有说明时附在冒号后。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn close_reason(&self) -> &str {
        &self.close_reason
    }

    /// 返回已校验的替代工作项 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 重复关闭时返回替代工作项 ID；误派关闭返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn replacement_work_item_id(&self) -> Option<&str> {
        self.replacement_work_item_id.as_deref()
    }

    /// 构造与本决策一致的强类型证据引用。
    ///
    /// # 参数
    /// * `work_item_id` - 当前工作项 ID。
    /// * `command_receipt_id` - 领域命令回执 ID。
    ///
    /// # 返回
    /// 返回引用当前工作项、本决策的替代工作项和命令回执的证据。
    ///
    /// # 错误
    /// 工作项或回执为空、含分号，或替代工作项与当前工作项相同时返回领域错误。
    pub fn evidence_reference(
        &self,
        work_item_id: &str,
        command_receipt_id: &str,
    ) -> Result<W29EvidenceReference> {
        let work_item_id = required_reference(Some(work_item_id), "工作项 ID 不能为空")?;
        let command_receipt_id = required_reference(Some(command_receipt_id), "命令回执 ID 不能为空")?;
        if self.replacement_work_item_id.as_deref() == Some(work_item_id.as_str()) {
            return Err(Error::from("替代任务不能引用自身"));
        }
        Ok(W29EvidenceReference {
            work_item_id,
            replacement_work_item_id: self.replacement_work_item_id.clone(),
            command_receipt_id,
        })
    }

    /// 计算下一条不可变差异决定序号。
    ///
    /// # 参数
    /// * `latest_resolution_no` - 当前最后一条决定序号；尚无决定时为 `None`。
    ///
    /// # 返回
    /// 尚无决定时返回 1，否则返回当前序号加一。
    ///
    /// # 错误
    /// `u32` 加一溢出时返回 [`RESOLUTION_NO_OVERFLOW_MESSAGE`] 对应的领域错误。
    pub fn next_resolution_no(latest_resolution_no: Option<u32>) -> Result<u32> {
        latest_resolution_no.map_or(Ok(1), |value| {
            value.checked_add(1).ok_or_else(|| Error::from(RESOLUTION_NO_OVERFLOW_MESSAGE))
        })
    }
}

/// 拒绝空白引用；ID 含分号时无法再按段解析。
fn required_reference(value: Option<&str>, message: &str) -> Result<String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    let value = value.ok_or_else(|| Error::from(message))?;
    if value.contains(';') {
        return Err(Error::from("证据引用 ID 不得包含分号"));
    }
    Ok(value.to_string())
}

/// 去掉固定前缀后按引用 ID 校验；没有该前缀视为格式非法。
fn reference_value(field: &str, prefix: &str) -> Result<String> {
    required_reference(field.strip_prefix(prefix), "领域关闭证据引用格式非法")
}

#[cfg(test)]
mod tests {
    use super::{W29CloseDecision, W29EvidenceReference};
    use crate::entity::integration_ops::ResolutionAction;

    #[test]
    fn duplicate_requires_replacement_and_round_trips_historical_evidence() {
        let decision = W29CloseDecision::new(" DUPLICATE ", Some(" 已有有效替代 "), Some("wi-2")).unwrap();
        assert_eq!(decision.resolution_action(), ResolutionAction::CloseDuplicate);
        assert_eq!(decision.close_reason(), "DUPLICATE replacement=wi-2: 已有有效替代");
        let evidence = decision.evidence_reference("wi-1", "audit-1").unwrap();
        let encoded = evidence.to_string();
        assert_eq!(encoded, "work_item:wi-1;replacement_work_item:wi-2;command_receipt:audit-1");
        assert_eq!(W29EvidenceReference::parse(&encoded).unwrap(), evidence);
    }

    #[test]
    fn misrouted_requires_comment_and_forbids_replacement() {
        assert!(W29CloseDecision::new("MISROUTED", None, None).is_err());
        assert!(W29CloseDecision::new("MISROUTED", Some("误派"), Some("wi-2")).is_err());
        let decision = W29CloseDecision::new("MISROUTED", Some("对象类型登记错误"), None).unwrap();
        assert_eq!(decision.resolution_action(), ResolutionAction::CloseMisrouted);
        assert_eq!(
            decision.evidence_reference("wi-1", "audit-2").unwrap().to_string(),
            "work_item:wi-1;command_receipt:audit-2"
        );
    }

    #[test]
    fn invalid_combinations_self_reference_and_sequence_overflow_fail_closed() {
        assert!(W29CloseDecision::new("DUPLICATE", None, None).is_err());
        assert!(W29CloseDecision::new("UNKNOWN", Some("原因"), None).is_err());
        let decision = W29CloseDecision::new("DUPLICATE", None, Some("wi-1")).unwrap();
        assert!(decision.evidence_reference("wi-1", "audit-1").is_err());
        assert!(W29EvidenceReference::parse("work_item:wi-1;command_receipt:").is_err());
        assert_eq!(W29CloseDecision::next_resolution_no(None).unwrap(), 1);
        assert_eq!(W29CloseDecision::next_resolution_no(Some(7)).unwrap(), 8);
        assert!(W29CloseDecision::next_resolution_no(Some(u32::MAX)).is_err());
    }
}
