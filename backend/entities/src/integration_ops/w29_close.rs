//! W29 受控关闭命令的纯决策与证据引用。

use std::fmt;

use crate::errors::{Error, Result};

use super::ResolutionAction;

/// W29 关闭证据的强类型引用。
///
/// 持久化继续使用历史分号格式，以保证存量记录可读且无需迁移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct W29EvidenceReference {
    work_item_id: String,
    replacement_work_item_id: Option<String>,
    audit_log_id: String,
}

impl W29EvidenceReference {
    /// 解析历史持久化格式。
    pub fn parse(value: &str) -> Result<Self> {
        let fields = value.trim().split(';').collect::<Vec<_>>();
        match fields.as_slice() {
            [work_item, audit] => Ok(Self {
                work_item_id: reference_value(work_item, "work_item:")?,
                replacement_work_item_id: None,
                audit_log_id: reference_value(audit, "audit_log:")?,
            }),
            [work_item, replacement, audit] => Ok(Self {
                work_item_id: reference_value(work_item, "work_item:")?,
                replacement_work_item_id: Some(reference_value(replacement, "replacement_work_item:")?),
                audit_log_id: reference_value(audit, "audit_log:")?,
            }),
            _ => Err(Error::from("领域关闭证据引用格式非法")),
        }
    }

    /// 返回证据对应的关闭动作。
    pub fn resolution_action(&self) -> ResolutionAction {
        if self.replacement_work_item_id.is_some() {
            ResolutionAction::CloseDuplicate
        } else {
            ResolutionAction::CloseMisrouted
        }
    }

    /// 返回当前工作项 ID。
    pub fn work_item_id(&self) -> &str {
        &self.work_item_id
    }

    /// 返回替代工作项 ID。
    pub fn replacement_work_item_id(&self) -> Option<&str> {
        self.replacement_work_item_id.as_deref()
    }

    /// 返回审计日志 ID。
    pub fn audit_log_id(&self) -> &str {
        &self.audit_log_id
    }
}

impl fmt::Display for W29EvidenceReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.replacement_work_item_id {
            Some(replacement) => write!(
                formatter,
                "work_item:{};replacement_work_item:{};audit_log:{}",
                self.work_item_id, replacement, self.audit_log_id
            ),
            None => write!(
                formatter,
                "work_item:{};audit_log:{}",
                self.work_item_id, self.audit_log_id
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
            }
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
            }
            _ => Err(Error::from("关闭原因只允许 DUPLICATE 或 MISROUTED")),
        }
    }

    /// 返回领域决定动作。
    pub fn resolution_action(&self) -> ResolutionAction {
        self.resolution_action
    }

    /// 返回规范化的工作项关闭原因。
    pub fn close_reason(&self) -> &str {
        &self.close_reason
    }

    /// 返回已校验的替代工作项 ID。
    pub fn replacement_work_item_id(&self) -> Option<&str> {
        self.replacement_work_item_id.as_deref()
    }

    /// 构造与本决策一致的强类型证据引用。
    pub fn evidence_reference(&self, work_item_id: &str, audit_log_id: &str) -> Result<W29EvidenceReference> {
        let work_item_id = required_reference(Some(work_item_id), "工作项 ID 不能为空")?;
        let audit_log_id = required_reference(Some(audit_log_id), "审计日志 ID 不能为空")?;
        if self.replacement_work_item_id.as_deref() == Some(work_item_id.as_str()) {
            return Err(Error::from("替代任务不能引用自身"));
        }
        Ok(W29EvidenceReference {
            work_item_id,
            replacement_work_item_id: self.replacement_work_item_id.clone(),
            audit_log_id,
        })
    }

    /// 计算下一条不可变差异决定序号。
    pub fn next_resolution_no(latest_resolution_no: Option<u32>) -> Result<u32> {
        latest_resolution_no.map_or(Ok(1), |value| {
            value
                .checked_add(1)
                .ok_or_else(|| Error::from("差异决定序号已达上限"))
        })
    }
}

fn required_reference(value: Option<&str>, message: &str) -> Result<String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    let value = value.ok_or_else(|| Error::from(message))?;
    if value.contains(';') {
        return Err(Error::from("证据引用 ID 不得包含分号"));
    }
    Ok(value.to_string())
}

fn reference_value(field: &str, prefix: &str) -> Result<String> {
    required_reference(field.strip_prefix(prefix), "领域关闭证据引用格式非法")
}

#[cfg(test)]
mod tests {
    use super::{W29CloseDecision, W29EvidenceReference};
    use crate::integration_ops::ResolutionAction;

    #[test]
    fn duplicate_requires_replacement_and_round_trips_historical_evidence() {
        let decision = W29CloseDecision::new(" DUPLICATE ", Some(" 已有有效替代 "), Some("wi-2")).unwrap();
        assert_eq!(decision.resolution_action(), ResolutionAction::CloseDuplicate);
        assert_eq!(
            decision.close_reason(),
            "DUPLICATE replacement=wi-2: 已有有效替代"
        );
        let evidence = decision.evidence_reference("wi-1", "audit-1").unwrap();
        let encoded = evidence.to_string();
        assert_eq!(
            encoded,
            "work_item:wi-1;replacement_work_item:wi-2;audit_log:audit-1"
        );
        assert_eq!(W29EvidenceReference::parse(&encoded).unwrap(), evidence);
    }

    #[test]
    fn misrouted_requires_comment_and_forbids_replacement() {
        assert!(W29CloseDecision::new("MISROUTED", None, None).is_err());
        assert!(W29CloseDecision::new("MISROUTED", Some("误派"), Some("wi-2")).is_err());
        let decision = W29CloseDecision::new("MISROUTED", Some("对象类型登记错误"), None).unwrap();
        assert_eq!(decision.resolution_action(), ResolutionAction::CloseMisrouted);
        assert_eq!(
            decision
                .evidence_reference("wi-1", "audit-2")
                .unwrap()
                .to_string(),
            "work_item:wi-1;audit_log:audit-2"
        );
    }

    #[test]
    fn invalid_combinations_self_reference_and_sequence_overflow_fail_closed() {
        assert!(W29CloseDecision::new("DUPLICATE", None, None).is_err());
        assert!(W29CloseDecision::new("UNKNOWN", Some("原因"), None).is_err());
        let decision = W29CloseDecision::new("DUPLICATE", None, Some("wi-1")).unwrap();
        assert!(decision.evidence_reference("wi-1", "audit-1").is_err());
        assert!(W29EvidenceReference::parse("work_item:wi-1;audit_log:").is_err());
        assert_eq!(W29CloseDecision::next_resolution_no(None).unwrap(), 1);
        assert_eq!(W29CloseDecision::next_resolution_no(Some(7)).unwrap(), 8);
        assert!(W29CloseDecision::next_resolution_no(Some(u32::MAX)).is_err());
    }
}
