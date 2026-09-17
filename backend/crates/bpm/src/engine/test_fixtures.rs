//! 引擎测试共享夹具（仅 `cfg(test)`），收敛三处重复的构图与资格 helper。
//!
//! 图级 `two_node_graph` 因 `ProcessKind`/节点名不同仍保留在各文件；
//! 此处只共享完全同形的原子 helper：`participant`/`at`/`eligible`/`blocked`/`node`。

use super::Eligibility;
use crate::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
use crate::model::types::ApprovalBlockerCode;
use crate::model::{ApprovalNodeDefinition, ParticipantId, Timestamp};

/// 构造处理人引用。
pub(crate) fn participant(id: &str) -> ParticipantId {
    ParticipantId::new(id).unwrap()
}

/// 构造调用方时间。
pub(crate) fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_secs(secs).unwrap()
}

/// 构造有效资格。
pub(crate) fn eligible(user: &str, name: &str) -> Eligibility {
    Eligibility::Eligible { participant: participant(user), assignee_name_snapshot: name.into() }
}

/// 构造受阻资格。
pub(crate) fn blocked(user: &str, name: &str, code: ApprovalBlockerCode) -> Eligibility {
    Eligibility::Blocked { participant: participant(user), code, assignee_name_snapshot: name.into() }
}

/// 构造定义节点（调用方提供全部身份与展示字段）。
pub(crate) fn node(
    id: &str,
    key: &str,
    name: &str,
    order: u32,
    user: &str,
    label: &str,
    at: Timestamp,
) -> ApprovalNodeDefinition {
    ApprovalNodeDefinition::new(crate::model::NewNodeDefinition {
        id: ApprovalNodeDefinitionId::new(id),
        process_definition_id: ApprovalProcessDefinitionId::new("def"),
        node_key: key.into(),
        node_name: name.into(),
        node_purpose: None,
        display_order: order,
        assignee_participant_id: participant(user),
        assignee_label_snapshot: label.into(),
        at,
    })
    .unwrap()
}
