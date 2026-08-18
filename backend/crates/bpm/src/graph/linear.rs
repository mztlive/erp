//! 由有序节点确定性生成线性连线。
//!
//! 完整图可达性、发布完整性由后续阶段聚合校验；本模块只按顺序生成连线草稿。

use crate::model::types::{
    ApprovalTerminalResult, ApprovalTransitionEvent, ModelError, ModelResult, NODE_KEY_MAX_LEN,
};

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

/// 按节点顺序生成线性通过链与驳回到首节点的连线。
///
/// 调用方传入的第一个键视为入口。本函数不读取定义实体，也不判断运行时末节点。
///
/// # 参数
/// * `node_keys` - 从入口开始的有序节点键
///
/// # 返回
/// 每个节点一条通过连线与一条驳回连线。
///
/// # 错误
/// 节点为空、键非法或存在重复时返回错误。
pub fn generate_linear_transitions(node_keys: &[String]) -> ModelResult<Vec<LinearTransitionDraft>> {
    let keys = normalize_keys(node_keys)?;
    let entry = keys[0].clone();
    let last = keys.len() - 1;
    let mut drafts = Vec::with_capacity(keys.len() * 2);
    for (index, from) in keys.iter().enumerate() {
        if index == last {
            drafts.push(LinearTransitionDraft {
                from_node_key: from.clone(),
                event: ApprovalTransitionEvent::Approve,
                to_node_key: None,
                terminal_result: Some(ApprovalTerminalResult::Approved),
            });
        } else {
            drafts.push(LinearTransitionDraft {
                from_node_key: from.clone(),
                event: ApprovalTransitionEvent::Approve,
                to_node_key: Some(keys[index + 1].clone()),
                terminal_result: None,
            });
        }
        drafts.push(LinearTransitionDraft {
            from_node_key: from.clone(),
            event: ApprovalTransitionEvent::Reject,
            to_node_key: Some(entry.clone()),
            terminal_result: None,
        });
    }
    Ok(drafts)
}

/// 规范化并拒绝空、超长与重复节点键。
///
/// # 错误
/// 节点集合不合法时返回 [`ModelError::InvalidField`]。
fn normalize_keys(node_keys: &[String]) -> ModelResult<Vec<String>> {
    if node_keys.is_empty() {
        return Err(ModelError::InvalidField("线性流程至少需要一个节点"));
    }
    let mut seen = Vec::with_capacity(node_keys.len());
    let mut keys = Vec::with_capacity(node_keys.len());
    for key in node_keys {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return Err(ModelError::InvalidField("节点键不能为空"));
        }
        if trimmed.len() > NODE_KEY_MAX_LEN {
            return Err(ModelError::InvalidField("节点键过长"));
        }
        if seen.iter().any(|item: &String| item == trimmed) {
            return Err(ModelError::InvalidField("节点键不能重复"));
        }
        seen.push(trimmed.to_string());
        keys.push(trimmed.to_string());
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::generate_linear_transitions;
    use crate::model::types::{ApprovalTerminalResult, ApprovalTransitionEvent};

    /// 单节点：通过进终态，驳回回自身。
    #[test]
    fn single_node_approves_to_terminal() {
        let drafts = generate_linear_transitions(&["n1".to_string()]).unwrap();
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].event, ApprovalTransitionEvent::Approve);
        assert_eq!(drafts[0].terminal_result, Some(ApprovalTerminalResult::Approved));
        assert_eq!(drafts[1].event, ApprovalTransitionEvent::Reject);
        assert_eq!(drafts[1].to_node_key.as_deref(), Some("n1"));
    }

    /// 多节点按顺序通过，驳回一律指向第一个键。
    #[test]
    fn linear_chain_rejects_to_first_key() {
        let drafts = generate_linear_transitions(&[" n1 ".to_string(), "n2".to_string()]).unwrap();
        assert_eq!(drafts.len(), 4);
        assert_eq!(drafts[0].to_node_key.as_deref(), Some("n2"));
        assert_eq!(drafts[2].terminal_result, Some(ApprovalTerminalResult::Approved));
        assert!(drafts
            .iter()
            .filter(|item| item.event == ApprovalTransitionEvent::Reject)
            .all(|item| item.to_node_key.as_deref() == Some("n1")));
    }

    /// 空列表与重复键失败关闭。
    #[test]
    fn rejects_empty_or_duplicate_keys() {
        assert!(generate_linear_transitions(&[]).is_err());
        assert!(generate_linear_transitions(&["n1".into(), "n1".into()]).is_err());
        assert!(generate_linear_transitions(&["  ".into()]).is_err());
    }
}
