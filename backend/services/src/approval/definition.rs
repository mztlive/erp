//! 审批流程定义管理：草稿、节点替换、发布、退役与版本查询。
//!
//! 图算法只编排 `bpm::graph` 已交付原语，不在本模块复制或另立定义源。

use mongodb::Database;

use erp_identity::SharedRbacService;

mod command;
mod create;
mod mapping;
mod publish;
mod query;
mod replace;
mod retire;

pub use super::scope::{definition_management_visibility, DefinitionManagementVisibility};

/// 审批流程定义管理服务。
pub struct ApprovalDefinitionService {
    db: Database,
    rbac: SharedRbacService,
}

impl ApprovalDefinitionService {
    /// 创建定义管理服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - 共享 RBAC 服务
    ///
    /// # 返回
    /// 返回尚未接线 HTTP 的应用端口。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 返回定义管理使用的数据库。
    ///
    /// # 返回
    /// 返回 MongoDB 句柄。
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

    pub fn production_source() -> &'static str {
        static SOURCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        SOURCE
            .get_or_init(|| {
                fn production_part(source: &str) -> &str {
                    source.split("#[cfg(test)]").next().expect("必须存在生产代码")
                }
                [
                    production_part(include_str!("definition.rs")),
                    production_part(include_str!("definition/query.rs")),
                    production_part(include_str!("definition/create.rs")),
                    production_part(include_str!("definition/replace.rs")),
                    production_part(include_str!("definition/publish.rs")),
                    production_part(include_str!("definition/retire.rs")),
                    production_part(include_str!("definition/mapping.rs")),
                    production_part(include_str!("definition/command.rs")),
                ]
                .concat()
            })
            .as_str()
    }

    pub fn source_fn<'a>(source: &'a str, name: &str, next: &str) -> &'a str {
        source
            .split(name)
            .nth(1)
            .and_then(|body| body.split(next).next())
            .unwrap_or(source)
    }

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
    use super::super::policy::{policy_of, ALL_DOCUMENT_TYPES};
    use super::super::process_kind::{document_type_of, process_kind_of};
    use super::test_support::production_source;

    /// 政策与 ProcessKind 映射穷尽，Service 无 BPM 第二定义源。
    #[test]
    fn policy_mapping_is_exhaustive_and_service_has_no_second_bpm_source() {
        for document_type in ALL_DOCUMENT_TYPES {
            let process_kind = process_kind_of(document_type);
            assert_eq!(document_type_of(process_kind), document_type);
            let _ = policy_of(document_type).unwrap();
        }
        let production = production_source();
        assert!(production.contains("validate_linear"));
        assert!(production.contains("plan_replacement_nodes"));
        assert!(!production.contains("generate_linear_transitions"));
        assert!(!production.contains("validate_transition"));
        assert!(!production.contains("validate_entry_node"));
        assert!(!production.contains("entities::approval::"));
        assert!(!production.contains(&format!("{}{}", "CARD_", "SALES_APPROVAL")));
        assert!(!production.contains("validate_definition("));
        assert!(!production.contains("access_control::DataScope"));
        assert!(!production.contains("approval_management_scope"));
    }
}
