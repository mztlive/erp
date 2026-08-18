//! 单条连线形状与图入口校验。
//!
//! 全部驳回回入口、末节点通过进终态等完整图规则不在本模块识别。

use crate::model::types::{ModelError, ModelResult, NODE_KEY_MAX_LEN};
use crate::model::ApprovalTransitionDefinition;

/// 校验单条连线实体形状。
///
/// 本函数不把来源节点解释为入口或末节点。
///
/// # 参数
/// * `transition` - 已构造的连线
///
/// # 错误
/// 形状不合法时返回 [`ModelError::InvalidTransition`]。
pub fn validate_transition(transition: &ApprovalTransitionDefinition) -> ModelResult<()> {
    transition.validate_shape()
}

/// 校验入口键存在于已给出的节点键集合。
///
/// # 参数
/// * `entry_node_key` - 定义入口
/// * `node_keys` - 定义内节点键
///
/// # 错误
/// 入口为空、超长或不在集合内时返回错误。
pub fn validate_entry_node(entry_node_key: &str, node_keys: &[String]) -> ModelResult<()> {
    let entry = entry_node_key.trim();
    if entry.is_empty() {
        return Err(ModelError::InvalidField("入口节点键不能为空"));
    }
    if entry.len() > NODE_KEY_MAX_LEN {
        return Err(ModelError::InvalidField("入口节点键过长"));
    }
    if node_keys.iter().any(|key| key.trim() == entry) {
        return Ok(());
    }
    Err(ModelError::InvalidField("入口节点必须存在于节点集合"))
}

#[cfg(test)]
mod tests {
    use super::{validate_entry_node, validate_transition};
    use crate::ids::{ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
    use crate::model::types::ApprovalTransitionEvent;
    use crate::model::{ApprovalTransitionDefinition, Timestamp};

    /// 合法连线通过；入口必须命中节点集合。
    #[test]
    fn validates_transition_and_entry() {
        let transition = ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new("t1"),
            ApprovalProcessDefinitionId::new("def"),
            "n1",
            ApprovalTransitionEvent::Approve,
            "n2",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        assert!(validate_transition(&transition).is_ok());
        assert!(validate_entry_node("n1", &["n1".into(), "n2".into()]).is_ok());
        assert!(validate_entry_node("n3", &["n1".into(), "n2".into()]).is_err());
        assert!(validate_entry_node("  ", &["n1".into()]).is_err());
    }
}
