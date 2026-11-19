//! 有界导出真实显式配置，不把默认或退休政策编译成新授权。
use application_core::AuditActor;
use erp_permissions::access_control;
use persistence_core::{Executor, Transactional};

use super::PolicyBundleService;
use crate::dto::authorization_bundle::{ExportPolicyRequest, PolicyExport};
use crate::entity::authorization_bundle::PolicyDocument;
use crate::{Error, Result};

impl PolicyBundleService {
    /// 导出显式选中的角色和人员的当前配置，供后续预览。角色和人员各最多选择 100 个。
    ///
    /// 按已认证身份校验权限，返回配置文件、策略版本，并单独说明不能保存为追加项的系统规则。
    /// 越权、目标失效、旧配置无法无损表达或存在角色继承时拒绝导出。
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

    // 导出时在同一只读快照中授权、取数并生成文件，交付前复验策略快照、读取阶段和有效期。
    async fn export_steps(
        &self,
        selection: PolicyDocument,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PolicyExport> {
        let authorization = self
            .access
            .authorization
            .require_policy_bundle(actor, &access_control::POLICY_BUNDLE_EXPORT, None, executor)
            .await?;

        let facts = self.facts(&selection, executor).await?;
        if selection.roles.iter().any(|r| !facts.roles.contains_key(r.id.as_str())) {
            return Err(Error::NotFound("所选角色不存在".into()));
        }
        let output = facts.export(authorization.scope.policy_version)?;
        output.document.validate_catalog(&self.catalog)?;

        self.access
            .rbac
            .clone()
            .ensure_policy_snapshot_with_executor(authorization.scope.policy_version, executor)
            .await?;
        authorization.ensure_current(executor)?;
        Ok(output)
    }
}
