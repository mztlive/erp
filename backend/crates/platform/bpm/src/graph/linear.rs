//! 由有序节点确定性生成线性连线。
//!
//! 完整图可达性、发布完整性由后续阶段聚合校验；本模块只按顺序生成连线草稿。

use std::collections::HashSet;

use crate::ids::{ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use crate::model::types::{
    ApprovalTerminalResult, ApprovalTransitionEvent, ModelError, ModelResult, NODE_KEY_MAX_LEN, trim_required,
};
use crate::model::{ApprovalNodeDefinition, ApprovalTransitionDefinition, Timestamp};

/// 尚未赋予主键的线性连线草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearTransitionDraft {
    /// 事件来源节点。
    pub from_node_key: String,
    /// 通过或驳回。
    pub event: ApprovalTransitionEvent,
    /// 指向下一节点时存在。
    pub to_node_key: Option<String>,
    /// 指向终态时存在。
    pub terminal_result: Option<ApprovalTerminalResult>,
}

/// 用从入口起排列的节点键生成线性连线草稿，供新图构建和既有图校验使用。
/// 每个节点各有一条通过边和驳回边：通过去往下一个节点或最终通过，驳回一律终结本实例。此时不分配 ID，也不读取定义实体。
///
/// # 错误
/// 节点键列表为空、键非法或规范化后重复时返回错误。
pub fn generate_linear_transitions(node_keys: &[String]) -> ModelResult<Vec<LinearTransitionDraft>> {
    let keys = normalize_keys(node_keys)?;

    let mut drafts = Vec::with_capacity(keys.len() * 2);
    for (index, from) in keys.iter().enumerate() {
        let next = keys.get(index + 1);
        drafts.push(LinearTransitionDraft {
            from_node_key: (*from).to_string(),
            event: ApprovalTransitionEvent::Approve,
            to_node_key: next.map(|key| (*key).to_string()),
            terminal_result: next.is_none().then_some(ApprovalTerminalResult::Approved),
        });
        drafts.push(LinearTransitionDraft {
            from_node_key: (*from).to_string(),
            event: ApprovalTransitionEvent::Reject,
            to_node_key: None,
            terminal_result: Some(ApprovalTerminalResult::Rejected),
        });
    }
    Ok(drafts)
}

/// 为新建或重建草稿按展示顺序生成连线实体，并依生成顺序为每条边使用调用方提供的 ID 与时间。
/// 连线形态由线性生成器决定；节点顺序非法、ID 数量不匹配或实体校验失败时返回模型错误。
pub fn build_linear_transitions(
    definition_id: &ApprovalProcessDefinitionId,
    nodes: &[ApprovalNodeDefinition],
    transition_ids: Vec<ApprovalTransitionDefinitionId>,
    at: Timestamp,
) -> ModelResult<Vec<ApprovalTransitionDefinition>> {
    let keys = super::ordered_nodes(nodes)?.into_iter().map(|node| node.node_key.clone()).collect::<Vec<_>>();
    let drafts = generate_linear_transitions(&keys)?;
    if drafts.len() != transition_ids.len() {
        return Err(ModelError::InvalidField("线性连线身份数量不匹配"));
    }

    drafts
        .into_iter()
        .zip(transition_ids)
        .map(|(draft, transition_id)| {
            if let Some(to_node_key) = draft.to_node_key {
                return ApprovalTransitionDefinition::to_node(
                    transition_id,
                    definition_id.clone(),
                    draft.from_node_key,
                    draft.event,
                    to_node_key,
                    at,
                );
            }
            if draft.event == ApprovalTransitionEvent::Reject {
                return ApprovalTransitionDefinition::to_rejected(
                    transition_id,
                    definition_id.clone(),
                    draft.from_node_key,
                    at,
                );
            }
            ApprovalTransitionDefinition::to_approved(
                transition_id,
                definition_id.clone(),
                draft.from_node_key,
                draft.event,
                at,
            )
        })
        .collect()
}

// 生成连线前借用裁剪后的键，按 UTF-8 字节长度校验，并将规范化后相同的键视为重复；空集合或非法键返回 `ModelError::InvalidField`。
fn normalize_keys(node_keys: &[String]) -> ModelResult<Vec<&str>> {
    if node_keys.is_empty() {
        return Err(ModelError::InvalidField("线性流程至少需要一个节点"));
    }

    let mut seen = HashSet::with_capacity(node_keys.len());
    let mut keys = Vec::with_capacity(node_keys.len());
    for key in node_keys {
        let normalized = trim_required(key, "节点键不能为空", NODE_KEY_MAX_LEN, "节点键过长")?;
        if !seen.insert(normalized) {
            return Err(ModelError::InvalidField("节点键不能重复"));
        }
        keys.push(normalized);
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::generate_linear_transitions;
    use crate::model::types::{
        ApprovalTerminalResult, ApprovalTransitionEvent, ModelError, NODE_KEY_MAX_LEN,
    };

    // 验证单节点既是入口也是末节点，两种决定都不会再生成节点目标。
    #[test]
    fn single_node_approves_to_terminal() {
        let drafts = generate_linear_transitions(&["n1".to_string()]).unwrap();
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].event, ApprovalTransitionEvent::Approve);
        assert_eq!(drafts[0].terminal_result, Some(ApprovalTerminalResult::Approved));
        assert_eq!(drafts[1].event, ApprovalTransitionEvent::Reject);
        assert!(drafts[1].to_node_key.is_none());
        assert_eq!(drafts[1].terminal_result, Some(ApprovalTerminalResult::Rejected));
    }

    // 验证多节点只沿通过边逐级前进，任一节点驳回都不回到入口。
    #[test]
    fn linear_chain_rejects_to_terminal() {
        let drafts = generate_linear_transitions(&[" n1 ".to_string(), "n2".to_string()]).unwrap();
        assert_eq!(drafts.len(), 4);
        assert_eq!(drafts[0].to_node_key.as_deref(), Some("n2"));
        assert_eq!(drafts[2].terminal_result, Some(ApprovalTerminalResult::Approved));
        assert!(
            drafts
                .iter()
                .filter(|item| item.event == ApprovalTransitionEvent::Reject)
                .all(|item| item.to_node_key.is_none()
                    && item.terminal_result == Some(ApprovalTerminalResult::Rejected))
        );
    }

    // 验证无节点或键重复时不生成无法定位来源的连线。
    #[test]
    fn rejects_empty_or_duplicate_keys() {
        assert!(generate_linear_transitions(&[]).is_err());
        assert!(generate_linear_transitions(&["n1".into(), "n1".into()]).is_err());
        assert!(generate_linear_transitions(&["  ".into()]).is_err());
    }

    // 用多字节键验证长度按 UTF-8 字节计，并覆盖去除首尾空白后发生的键冲突。
    #[test]
    fn normalized_keys_preserve_utf8_byte_boundary_and_duplicate_detection() {
        let longest = "界".repeat(NODE_KEY_MAX_LEN / 3);
        assert!(generate_linear_transitions(&[format!(" {longest} ")]).is_ok());
        assert_eq!(
            generate_linear_transitions(&["界".repeat(NODE_KEY_MAX_LEN / 3 + 1)]),
            Err(ModelError::InvalidField("节点键过长"))
        );

        assert_eq!(
            generate_linear_transitions(&["\t 节点 \n".into(), "节点".into()]),
            Err(ModelError::InvalidField("节点键不能重复"))
        );
    }
}
