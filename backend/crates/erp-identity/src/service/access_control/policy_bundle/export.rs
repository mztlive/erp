//! 有界导出真实显式配置，不把默认或退休政策编译成新授权。
use application_core::AuditActor;
use persistence_core::{Executor, Transactional};

use super::PolicyBundleService;
use crate::dto::authorization_bundle::{ExportPolicyRequest, PolicyExport};
use crate::entity::authorization_bundle::PolicyDocument;
use crate::{Error, Result};

impl PolicyBundleService {
    /// 导出明确选择的角色及人员当前配置。
    /// # 参数
    /// request 为最多100个角色及100个人员的选择，actor 为认证身份。
    /// # 返回
    /// 可再次预览的文件、版本及不能保存为追加项的系统规则。
    /// # 错误
    /// 越权、目标失效、复杂旧配置或角色继承时拒绝。
    pub async fn export(&self, request: ExportPolicyRequest, actor: AuditActor) -> Result<PolicyExport> {
        let selection = request.selection()?;
        let service = self.clone();
        self.access
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.export_steps(selection, &actor, executor).await })
            })
            .await
    }

    /// 使用单个只读事务导出，并核对缓存与原快照一致。
    async fn export_steps(
        &self,
        selection: PolicyDocument,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PolicyExport> {
        let authorization = self
            .company_access(
                actor,
                &["authorization_policy:export", "admin:list", "role:list", "data_scope:list"],
                "list",
                executor,
            )
            .await?;
        let facts = self.facts(&selection, executor).await?;
        if selection.roles.iter().any(|r| !facts.roles.contains_key(r.id.as_str())) {
            return Err(Error::NotFound("所选角色不存在".into()));
        }
        let output = facts.export(authorization.policy_version)?;
        output.document.validate_catalog(&self.catalog)?;
        self.access
            .scope_rbac()?
            .ensure_policy_snapshot_with_executor(authorization.policy_version, executor)
            .await?;
        Ok(output)
    }
}
