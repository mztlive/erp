//! 草稿节点及完整或签人员集合的幂等身份。
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{ApprovalCommandIdentity, CanonicalCommandPayload, CommandPayloadField, IdempotencyKey};

use super::{PreparedDefinitionIdentity, REPLACE_NODES_COMMAND_DOMAIN, map_identity_error};
use crate::entity::approval_copy::ApprovalCopyRule;
use crate::error::{Error, Result};
use crate::service::approval::definition_dto::DefinitionNodeRequest;

/// 整组节点替换的 v3 身份；Create/Replace 即使资源文本相同也使用不同 domain。
///
/// # 参数
/// * `key` - 规范幂等键
/// * `definition_id` - 定义主键
/// * `expected_lock` - 期望的定义锁版本
/// * `nodes` - 整组节点请求
/// * `actor_id` - 操作人 ID
///
/// # 返回
/// 返回不含历史候选的当前 V3 身份。
///
/// # 错误
/// 身份构造失败时返回内部错误。
pub(in crate::service::approval::definition) fn replace_nodes_identity(
    key: IdempotencyKey,
    definition_id: &str,
    expected_lock: u64,
    nodes: &[DefinitionNodeRequest],
    copy_rules: Option<&[ApprovalCopyRule]>,
    actor_id: &str,
) -> Result<PreparedDefinitionIdentity> {
    let node_fields = nodes
        .iter()
        .map(|node| {
            let mut fields = vec![
                CommandPayloadField::OptionalText(node.node_id.as_deref()),
                CommandPayloadField::Text(&node.node_name),
                CommandPayloadField::U32(node.display_order),
                CommandPayloadField::Text(&node.assignee_user_id),
            ];
            if let Some(rule) = node.assignee_source.rule() {
                fields.push(CommandPayloadField::Text(rule));
            }
            if !node.additional_assignee_user_ids.is_empty() {
                fields.push(CommandPayloadField::Sequence(
                    node.additional_assignee_user_ids
                        .iter()
                        .map(|id| CommandPayloadField::Text(id))
                        .collect(),
                ));
            }
            CommandPayloadField::Sequence(fields)
        })
        .collect();
    let copy_payload = copy_rules
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| Error::Internal(error.to_string()))?;
    let mut payload = CanonicalCommandPayload::new()
        .field(CommandPayloadField::Text(definition_id))
        .field(CommandPayloadField::U64(expected_lock))
        .field(CommandPayloadField::Sequence(node_fields));
    if let Some(copies) = &copy_payload {
        payload = payload.field(CommandPayloadField::Text(copies));
    }
    let current = ApprovalCommandIdentity::new(
        ApprovalCommandKind::DefinitionWrite,
        REPLACE_NODES_COMMAND_DOMAIN,
        key,
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(definition_id)),
        payload.field(CommandPayloadField::Text(actor_id)),
    )
    .map_err(map_identity_error)?;
    Ok(PreparedDefinitionIdentity { current, legacy: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn leader_source_changes_command_digest() {
        use crate::service::approval::assignee_source::AssigneeSource;
        let key = IdempotencyKey::parse("leader-save").unwrap();
        let mut node = DefinitionNodeRequest::new("负责人审核".into(), String::new());
        node.assignee_source = AssigneeSource::ApplicantTeamLeader;
        let team = replace_nodes_identity(key.clone(), "def", 1, &[node.clone()], None, "admin").unwrap();
        node.assignee_source = AssigneeSource::ApplicantDepartmentLeader;
        let department = replace_nodes_identity(key, "def", 1, &[node], None, "admin").unwrap();
        assert_eq!(team.current.scope(), department.current.scope());
        assert_ne!(team.current.digest(), department.current.digest());
    }

    #[test]
    fn copy_rules_participate_in_retry_identity() {
        use crate::entity::approval_copy::{ApprovalCopyRule, CopyTrigger};
        let nodes = [DefinitionNodeRequest::new("审批".into(), "approver".into())];
        let key = IdempotencyKey::parse("copy-save").unwrap();
        let rules = [ApprovalCopyRule {
            trigger: CopyTrigger::Approved,
            node_order: None,
            recipient_user_ids: vec!["reader".into()],
        }];
        let first = replace_nodes_identity(key.clone(), "def", 1, &nodes, Some(&rules), "admin").unwrap();
        let replay = replace_nodes_identity(key.clone(), "def", 1, &nodes, Some(&rules), "admin").unwrap();
        let empty = replace_nodes_identity(key, "def", 1, &nodes, Some(&[]), "admin").unwrap();
        assert_eq!(first, replay);
        assert_eq!(first.current.scope(), empty.current.scope());
        assert_ne!(first.current.digest(), empty.current.digest());
    }
    #[test]
    fn or_sign_members_change_payload_but_keep_command_scope() {
        let key = IdempotencyKey::parse("or-sign").unwrap();
        let mut node = DefinitionNodeRequest::new("复核".into(), "u1".into());
        let single = replace_nodes_identity(key.clone(), "def", 1, &[node.clone()], None, "admin").unwrap();
        node.additional_assignee_user_ids.push("u2".into());
        let group = replace_nodes_identity(key.clone(), "def", 1, &[node.clone()], None, "admin").unwrap();
        let retry = replace_nodes_identity(key, "def", 1, &[node], None, "admin").unwrap();
        assert_eq!(single.current.scope(), group.current.scope());
        assert_ne!(single.current.digest(), group.current.digest());
        assert_eq!(group, retry);
    }
}
