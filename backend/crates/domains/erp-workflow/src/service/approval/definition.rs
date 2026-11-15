//! 审批流程定义管理：草稿、节点替换、发布、退役与版本查询。
//!
//! 图算法只编排 `bpm::graph` 已交付原语，不在本模块复制或另立定义源。

use std::sync::Arc;

use erp_identity::{AuthorizationService, SharedRbacService};
use mongodb::Database;

use crate::ports::{FailClosedAuditPort, WorkflowAuditPort};

mod command;
pub(super) use command::require_definition_access;
mod create;
mod mapping;
mod publish;
mod query;
mod replace;
mod retire;

pub use super::scope::{DefinitionManagementVisibility, definition_management_visibility};

/// 审批流程定义管理服务。
#[derive(Clone)]
pub struct ApprovalDefinitionService {
    pub(crate) identity: SharedRbacService,
    pub(crate) authorization: Arc<AuthorizationService>,
    pub(super) db: Database,
    pub(crate) audit: Arc<dyn WorkflowAuditPort>,
}

impl ApprovalDefinitionService {
    /// 创建定义管理服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `identity` - 共享身份与权限服务
    ///
    /// # 返回
    /// 返回尚未接线 HTTP 的应用端口。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, identity: SharedRbacService, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, identity, authorization, audit: Arc::new(FailClosedAuditPort) }
    }

    /// 创建注入审计端口的定义管理服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `identity` - 共享身份与权限服务
    /// * `audit` - 审计写入端口
    ///
    /// # 返回
    /// 返回使用给定审计端口的服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_audit(
        db: Database,
        identity: SharedRbacService,
        authorization: Arc<AuthorizationService>,
        audit: Arc<dyn WorkflowAuditPort>,
    ) -> Self {
        Self { db, identity, authorization, audit }
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use std::sync::Arc;

    use application_core::AuditActor;
    use async_trait::async_trait;
    use bpm::graph::DefinitionGraph;
    use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId};
    use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition};
    use bpm::{ParticipantId, ProcessKind, Timestamp};
    use erp_core::AccountKind;
    use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
    use erp_identity::testing::unavailable_authorization;
    use erp_identity::{RbacService, Result as IdentityResult};
    use mongodb::Client;
    use persistence_core::Executor;

    use super::ApprovalDefinitionService;
    use super::replace::next_transition_ids;

    struct NoAudit;
    #[async_trait]
    impl IdentityAuditPort for NoAudit {
        fn resource_log(
            &self,
            _: AuditActor,
            _: &str,
            _: &str,
            _: String,
        ) -> IdentityResult<PreparedResourceAudit> {
            panic!("授权拒绝前不得准备审计")
        }
        async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> IdentityResult<()> {
            panic!("授权拒绝前不得写入审计")
        }
    }

    /// 创建必须在任何业务或审计访问之前拒绝的定义服务。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不可用授权设施与当前认证账号。
    /// # 错误
    /// 不执行 I/O。
    pub async fn unavailable_service() -> (ApprovalDefinitionService, AuditActor) {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let db = client.database("definition_authorization_unit");
        let identity = Arc::new(RbacService::new(db.clone(), Arc::new(NoAudit)));
        let service = ApprovalDefinitionService::new(db, identity, unavailable_authorization());
        let actor = AuditActor::new("admin".into(), "admin".into(), AccountKind::Admin);
        (service, actor)
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
            assignee_participant_id: Some(ParticipantId::new(user).unwrap()),
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
