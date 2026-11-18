//! 应用注入的审批共有运行能力；具体授权和审计适配器不进入业务编排依赖图。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use bpm::engine::DefinitionGraph;
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_read_models::workbench::authorize_material_transfer;
use erp_support::FileAsset;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::ports::WorkflowAuditPort;
use erp_workflow::service::approval::binding::{
    BindPublishedDefinitionCommand, bind_published_definition_on_document_create, revalidate_binding_graph,
};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::copies::persist_copies;
use erp_workflow::service::approval::execution::PlannedWrites;
use erp_workflow::service::approval::policy::ProcessRequiredApprovalPolicy;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::approval_materials::authorize_attachment;
use crate::approval_start::persist_start_snapshot;

/// 业务命令消费的审批运行合同，所有事务步骤复用原执行器。
#[async_trait]
pub trait ApprovalRuntimePort: Send + Sync {
    /// 在原业务事务内重验并冻结提交材料，保存不可变快照和启动抄送，使后续任务引用本次正式提交的资料。授权或写入失败时停止。
    async fn persist_start_snapshot(
        &self,
        db: &Database,
        snapshot: ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 在原撤回事务内按计划事件、抄送规则和原快照登记抄送，使其与撤回结果一起提交。
    /// 快照不符、账号读取或抄送写入失败时返回原错误。
    async fn persist_cancel_copies(
        &self,
        db: &Database,
        writes: &PlannedWrites,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 用应用注入的授权和审计服务，为新单据绑定当前生效的审批定义。
    /// 无需审批时只登记策略事实并返回 `None`；必须审批但无有效定义或授权、审计失败时拒绝。
    async fn bind_document(
        &self,
        db: &Database,
        command: &BindPublishedDefinitionCommand,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalDefinitionBinding>>;

    /// 业务入口关联已有附件前，核验当前操作人是否为有权转授该资产的实际上传人；文件缺失、账号失效或无权转授时拒绝。
    async fn authorize_attachment(&self, db: &Database, actor: &AuditActor, file_id: &str) -> Result<()>;

    /// 在调用方事务中核验整组附件的转授资格，供提交步骤冻结材料；任何一项的材料或来源资格不成立都应停止并返回原错误。
    async fn authorize_material_transfer(
        &self,
        actor: &AuditActor,
        files: &[FileAsset],
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 沿原事务按单据类型和 ID 查询操作人是否可读审批来源，为材料访问提供业务授权依据。
    /// 无法取证或授权失败时返回错误，不当作正常否定结果。
    async fn source_readable(
        &self,
        actor: &AuditActor,
        document_type: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 写入前用原执行器复验绑定的精确审批定义、当前指派人和来源资格，防止沿用已失效的定义或权限。任一项不成立时拒绝继续。
    async fn revalidate_binding(
        &self,
        policy: &ProcessRequiredApprovalPolicy,
        context: &BindingRevalidationContext,
        graph: &DefinitionGraph,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

/// 由应用装配原授权与审计提供者的审批共有运行能力。
pub struct ApprovalRuntime<A> {
    db: Database,
    pub(crate) identity: Arc<AuthorizationService>,
    rbac: SharedRbacService,
    auth: A,
    audit: Arc<dyn WorkflowAuditPort>,
}

impl<A: WorkflowAuthorizationPort> ApprovalRuntime<A> {
    /// 绑定应用已有的数据库、共享身份与 RBAC、工作流授权及审计服务，供业务编排注入统一的审批运行端口；构造阶段不查询、不授权、不写入。
    pub fn new(
        db: Database,
        identity: Arc<AuthorizationService>,
        rbac: SharedRbacService,
        auth: A,
        audit: Arc<dyn WorkflowAuditPort>,
    ) -> Self {
        Self { db, identity, rbac, auth, audit }
    }
}

#[async_trait]
impl<A: WorkflowAuthorizationPort + Send + Sync> ApprovalRuntimePort for ApprovalRuntime<A> {
    async fn persist_start_snapshot(
        &self,
        db: &Database,
        snapshot: ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        persist_start_snapshot(&self.identity, &self.rbac, db, snapshot, &self.auth, executor).await
    }

    async fn persist_cancel_copies(
        &self,
        db: &Database,
        writes: &PlannedWrites,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        persist_copies(&self.rbac, db, &writes.instance, &writes.events, executor).await?;
        Ok(())
    }

    async fn bind_document(
        &self,
        db: &Database,
        command: &BindPublishedDefinitionCommand,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalDefinitionBinding>> {
        Ok(bind_published_definition_on_document_create(
            &self.identity,
            &self.rbac,
            db,
            self.audit.as_ref(),
            command,
            actor,
            executor,
        )
        .await?)
    }

    async fn authorize_attachment(&self, db: &Database, actor: &AuditActor, file_id: &str) -> Result<()> {
        authorize_attachment(&self.identity, db, actor, file_id).await
    }

    async fn authorize_material_transfer(
        &self,
        actor: &AuditActor,
        files: &[FileAsset],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(authorize_material_transfer(&self.identity, &self.db, actor, files, executor).await?)
    }
    async fn source_readable(
        &self,
        actor: &AuditActor,
        document_type: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.auth.approval_source_readable(actor, document_type, id, executor).await?)
    }

    async fn revalidate_binding(
        &self,
        policy: &ProcessRequiredApprovalPolicy,
        context: &BindingRevalidationContext,
        graph: &DefinitionGraph,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(revalidate_binding_graph(&self.identity, &self.db, &self.rbac, policy, context, graph, executor)
            .await?)
    }
}
