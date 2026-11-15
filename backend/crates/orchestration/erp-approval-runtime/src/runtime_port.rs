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
use erp_workflow::service::approval::execution::PlannedWrites;
use erp_workflow::service::approval::policy::ProcessRequiredApprovalPolicy;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::approval_materials::authorize_attachment;
use crate::approval_start::persist_start_snapshot;
use crate::material_writes::persist_cancel_copies;

/// 业务命令消费的审批运行合同，所有事务步骤复用原执行器。
#[async_trait]
pub trait ApprovalRuntimePort: Send + Sync {
    /// 在原事务内冻结并保存审批提交快照。
    /// # 参数
    /// `db`、`snapshot` 和 `executor` 分别为原数据库、提交快照和执行器。
    /// # 返回
    /// 快照及开始抄送保存成功时返回。
    /// # 错误
    /// 授权、材料冻结或持久化失败时返回原错误。
    async fn persist_start_snapshot(
        &self,
        db: &Database,
        snapshot: ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 在原撤回事务保存计划中的抄送。
    /// # 参数
    /// `db`、`writes` 和 `executor` 为原数据库、撤回计划和执行器。
    /// # 返回
    /// 计划抄送写入成功时返回。
    /// # 错误
    /// 来源授权或抄送持久化失败时返回原错误。
    async fn persist_cancel_copies(
        &self,
        db: &Database,
        writes: &PlannedWrites,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 用应用授权与审计提供者绑定已发布定义。
    /// # 参数
    /// `db`、`command`、`actor` 和 `executor` 为原绑定上下文。
    /// # 返回
    /// 返回原策略决定的绑定或无审批结果。
    /// # 错误
    /// 来源、授权、定义或审计写入失败时返回原错误。
    async fn bind_document(
        &self,
        db: &Database,
        command: &BindPublishedDefinitionCommand,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalDefinitionBinding>>;

    /// 核验单个既有附件的材料转授资格。
    /// # 参数
    /// `db`、`actor` 和 `file_id` 为原附件来源上下文。
    /// # 返回
    /// 读取及转授资格核验成功时返回。
    /// # 错误
    /// 资产不存在、账号或转授资格无效时返回原错误。
    async fn authorize_attachment(&self, db: &Database, actor: &AuditActor, file_id: &str) -> Result<()>;

    /// 在调用方执行器核验整组材料转授资格。
    /// # 参数
    /// `actor`、`files` 和 `executor` 为原提交身份、材料及执行器。
    /// # 返回
    /// 全组材料均可转授时返回。
    /// # 错误
    /// 任一材料或来源资格不成立时保持原错误并停止。
    async fn authorize_material_transfer(
        &self,
        actor: &AuditActor,
        files: &[FileAsset],
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 在原事务内证明操作人能够读取指定审批来源。
    /// # 参数
    /// `actor`、`document_type`、`id` 与 `executor` 为原来源读取上下文。
    /// # 返回
    /// 来源可读时返回 `true`。
    /// # 错误
    /// 来源读取或授权失败时返回原错误。
    async fn source_readable(
        &self,
        actor: &AuditActor,
        document_type: DocumentType,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 在原事务重验精确绑定定义、指派人和来源资格。
    /// # 参数
    /// `policy`、`context`、`graph` 和 `executor` 为原绑定重验上下文。
    /// # 返回
    /// 定义、当前人员和来源资格全部成立时返回。
    /// # 错误
    /// 任一绑定或人员资格失效时返回原错误。
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
    /// 绑定现有提供者；构造时不查询、授权或写入。
    /// # 参数
    /// `identity` 为共享身份服务；`auth`、`audit` 为业务授权和审计提供者。
    /// # 返回
    /// 返回可注入业务编排的运行能力。
    /// # 错误
    /// 不返回错误。
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
        persist_cancel_copies(&self.rbac, db, writes, executor).await
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
