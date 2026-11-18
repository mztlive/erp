//! 节点内冻结的审批参与人；多人节点按首个有效决定结束。
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::ParticipantId;
use super::types::{LABEL_MAX_LEN, ModelError, ModelResult, normalize_required, trim_required};

/// 审批参与人的身份与显示名快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalParticipant {
    /// 参与人身份。
    pub participant_id: ParticipantId,
    /// 显示名快照。
    pub name: String,
}

impl ApprovalParticipant {
    /// 把调用方已确定的审批人和显示名冻结为节点参与人快照，供后续执行展示，不再按实时账号名回查。
    /// 显示名去除首尾空白后不得为空，且不能超过 `LABEL_MAX_LEN` 个 UTF-8 字节。
    pub fn new(participant_id: ParticipantId, name: impl Into<String>) -> ModelResult<Self> {
        Ok(Self {
            participant_id,
            name: normalize_required(name, "审批人显示名不能为空", LABEL_MAX_LEN, "审批人显示名过长")?,
        })
    }
}

pub(crate) fn validate_additional(
    primary: &ParticipantId,
    additional: &[ApprovalParticipant],
) -> ModelResult<()> {
    if additional.len() > 49 {
        return Err(ModelError::InvalidField("每个节点最多配置 50 名审批人"));
    }
    let mut seen = HashSet::from([primary]);
    for item in additional {
        if !seen.insert(&item.participant_id) {
            return Err(ModelError::InvalidField("同一节点审批人不能重复"));
        }
        trim_required(&item.name, "审批人显示名不能为空", LABEL_MAX_LEN, "审批人显示名过长")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::*;

    #[test]
    fn additional_names_keep_trimmed_byte_limit_and_duplicate_error_priority() {
        let primary = ParticipantId::new("primary").unwrap();
        let mut person = ApprovalParticipant {
            participant_id: ParticipantId::new("secondary").unwrap(),
            name: format!("  {}  ", "人".repeat(LABEL_MAX_LEN / 3)),
        };
        validate_additional(&primary, slice::from_ref(&person)).unwrap();
        assert!(person.name.starts_with("  "));

        person.name = "人".repeat(LABEL_MAX_LEN / 3 + 1);
        assert_eq!(
            validate_additional(&primary, slice::from_ref(&person)),
            Err(ModelError::InvalidField("审批人显示名过长"))
        );
        person.name = " \t ".into();
        assert_eq!(
            validate_additional(&primary, slice::from_ref(&person)),
            Err(ModelError::InvalidField("审批人显示名不能为空"))
        );

        person.participant_id = primary.clone();
        assert_eq!(
            validate_additional(&primary, &[person]),
            Err(ModelError::InvalidField("同一节点审批人不能重复"))
        );
    }
}
