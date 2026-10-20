//! 草稿节点及完整或签人员集合的幂等身份。
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{ApprovalCommandIdentity, CanonicalCommandPayload, CommandPayloadField, IdempotencyKey};

use super::{PreparedDefinitionIdentity, REPLACE_NODES_COMMAND_DOMAIN, map_identity_error};
use crate::error::Result;
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
    let current = ApprovalCommandIdentity::new(
        ApprovalCommandKind::DefinitionWrite,
        REPLACE_NODES_COMMAND_DOMAIN,
        key,
        CanonicalCommandPayload::new().field(CommandPayloadField::Text(definition_id)),
        CanonicalCommandPayload::new()
            .field(CommandPayloadField::Text(definition_id))
            .field(CommandPayloadField::U64(expected_lock))
            .field(CommandPayloadField::Sequence(node_fields))
            .field(CommandPayloadField::Text(actor_id)),
    )
    .map_err(map_identity_error)?;
    Ok(PreparedDefinitionIdentity { current, legacy: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn or_sign_members_change_payload_but_keep_command_scope() {
        let key = IdempotencyKey::parse("or-sign").unwrap();
        let mut node = DefinitionNodeRequest::new("复核".into(), "u1".into());
        let single = replace_nodes_identity(key.clone(), "def", 1, &[node.clone()], "admin").unwrap();
        node.additional_assignee_user_ids.push("u2".into());
        let group = replace_nodes_identity(key.clone(), "def", 1, &[node.clone()], "admin").unwrap();
        let retry = replace_nodes_identity(key, "def", 1, &[node], "admin").unwrap();
        assert_eq!(single.current.scope(), group.current.scope());
        assert_ne!(single.current.digest(), group.current.digest());
        assert_eq!(group, retry);
    }
}
