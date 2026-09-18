//! 审批流程定义管理：草稿、节点替换、发布、退役与版本查询。
//!
//! 图算法只编排 `bpm::graph` 已交付原语，不在本模块复制或另立定义源。

use std::sync::Arc;

use mongodb::Database;

use crate::ports::{FailClosedAuditPort, WorkflowAuditPort, WorkflowAuthorizationPort};

mod command;
mod create;
mod mapping;
mod publish;
mod query;
mod replace;
mod retire;

pub use super::scope::{DefinitionManagementVisibility, definition_management_visibility};

/// 审批流程定义管理服务。
pub struct ApprovalDefinitionService<A> {
    db: Database,
    pub(crate) auth: A,
    pub(crate) audit: Arc<dyn WorkflowAuditPort>,
}

impl<A: WorkflowAuthorizationPort> ApprovalDefinitionService<A> {
    /// 创建定义管理服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `auth` - 授权 Port
    ///
    /// # 返回
    /// 返回尚未接线 HTTP 的应用端口。
    pub fn new(db: Database, auth: A) -> Self {
        Self { db, auth, audit: Arc::new(FailClosedAuditPort) }
    }

    /// Create a definition service with an injected audit port.
    pub fn with_audit(db: Database, auth: A, audit: Arc<dyn WorkflowAuditPort>) -> Self {
        Self { db, auth, audit }
    }

    /// 返回定义管理使用的数据库。
    ///
    /// # 返回
    /// 返回 MongoDB 句柄。
    #[allow(dead_code)]
    pub(crate) fn db(&self) -> &Database {
        &self.db
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use bpm::graph::DefinitionGraph;
    use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
    use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition};
    use bpm::{ParticipantId, ProcessKind, Timestamp};

    use super::replace::next_transition_ids;

    pub fn draft_definition(process_kind: ProcessKind, entry: &str) -> ApprovalProcessDefinition {
        ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def-1"),
            process_kind,
            1,
            "测试流程",
            entry,
            ParticipantId::new("admin").unwrap(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    fn node(id: &str, key: &str, order: u32, purpose: Option<&str>, user: &str) -> ApprovalNodeDefinition {
        ApprovalNodeDefinition::new(bpm::model::NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(id),
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            node_key: key.into(),
            node_name: format!("节点{order}"),
            node_purpose: purpose.map(ToOwned::to_owned),
            display_order: order,
            assignee_participant_id: ParticipantId::new(user).unwrap(),
            assignee_label_snapshot: "张三".to_string(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    pub fn two_node_publish_graph() -> DefinitionGraph {
        let nodes = vec![node("id1", "n1", 1, None, "u1"), node("id2", "n2", 2, None, "u2")];
        let definition = draft_definition(ProcessKind::StockAdjustment, "n1");
        DefinitionGraph::rebuild_draft(
            &definition,
            nodes,
            next_transition_ids(2),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::super::policy::{ALL_DOCUMENT_TYPES, policy_of};
    use super::super::process_kind::{document_type_of, process_kind_of};

    /// 政策与 ProcessKind 映射穷尽。
    #[test]
    fn policy_mapping_is_exhaustive() {
        for document_type in ALL_DOCUMENT_TYPES {
            let process_kind = process_kind_of(document_type);
            assert_eq!(document_type_of(process_kind), document_type);
            let _ = policy_of(document_type).unwrap();
        }
    }
}
