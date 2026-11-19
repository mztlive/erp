//! 有界导出真实显式配置，不把默认或退休政策编译成新授权。
use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_permissions::access_control;
use persistence_core::{Executor, Transactional};

use super::PolicyBundleService;
use crate::dto::authorization_bundle::{ExportPolicyRequest, PolicyExport};
use crate::entity::authorization_bundle::normalize_id;
use crate::entity::role::ROOT_ROLE_ID;
use crate::{Error, Result, RoleId};

impl PolicyBundleService {
    /// 导出显式选中的角色和人员的当前配置，供后续预览。角色和人员各最多选择 100 个。
    ///
    /// 按已认证身份校验权限，返回配置文件、策略版本，并单独说明不能保存为追加项的系统规则。
    /// 越权、目标失效、旧配置无法无损表达或存在角色继承时拒绝导出。
    pub async fn export(&self, request: ExportPolicyRequest, actor: AuditActor) -> Result<PolicyExport> {
        let (roles, users) = export_selection(request)?;
        let service = self.clone();
        self.access
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.export_steps(roles, users, &actor, executor).await })
            })
            .await
    }

    // 导出时在同一只读快照中授权、取数并生成文件，交付前复验策略快照、读取阶段和有效期。
    async fn export_steps(
        &self,
        roles: BTreeSet<String>,
        users: Vec<String>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PolicyExport> {
        let authorization = self
            .access
            .authorization
            .require_policy_bundle(actor, &access_control::POLICY_BUNDLE_EXPORT, None, executor)
            .await?;

        let facts = self.facts(roles.clone(), users, executor).await?;
        if roles.iter().any(|id| !facts.roles.contains_key(id)) {
            return Err(Error::NotFound("所选角色不存在".into()));
        }
        let output = facts.export(authorization.policy_version)?;
        output.document.validate_catalog(&self.catalog)?;

        self.access
            .rbac
            .clone()
            .ensure_policy_snapshot_with_executor(authorization.policy_version, executor)
            .await?;
        authorization.ensure_policy_bundle_current(executor)?;
        Ok(output)
    }
}

fn export_selection(request: ExportPolicyRequest) -> Result<(BTreeSet<String>, Vec<String>)> {
    if request.role_ids.len() > 100
        || request.user_ids.len() > 100
        || (request.role_ids.is_empty() && request.user_ids.is_empty())
    {
        return Err(Error::ValidationError("导出须选择1至100个角色或人员".into()));
    }

    let role_ids = request.role_ids.into_iter().map(RoleId::parse).collect::<erp_core::Result<Vec<_>>>()?;
    let mut roles = BTreeSet::new();
    for id in role_ids {
        if id.as_str() == ROOT_ROLE_ID || !roles.insert(id.to_string()) {
            return Err(Error::ValidationError("系统角色或重复角色不能写入策略文件".into()));
        }
    }

    let mut users = BTreeSet::new();
    for id in request.user_ids {
        if !users.insert(normalize_id(&id)?) {
            return Err(Error::ValidationError("重复人员绑定或角色数超过100".into()));
        }
    }
    Ok((roles, users.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_selection_keeps_sorted_ids_and_rejects_invalid_selections() {
        let select = |roles: &[&str], users: &[&str]| {
            export_selection(ExportPolicyRequest {
                role_ids: roles.iter().map(|id| (*id).into()).collect(),
                user_ids: users.iter().map(|id| (*id).into()).collect(),
            })
        };
        let (roles, users) = select(&["reader", "buyer"], &[" zoe ", "alice"]).unwrap();
        assert_eq!(roles.into_iter().collect::<Vec<_>>(), ["buyer", "reader"]);
        assert_eq!(users, ["alice", "zoe"]);
        assert!(select(&[], &[]).is_err());
        assert!(select(&[ROOT_ROLE_ID], &[]).is_err());
        assert!(select(&["reader", "reader"], &[]).is_err());
        assert!(select(&[], &["alice", " alice "]).is_err());
        assert!(select(&["reader"; 101], &[]).is_err());
    }
}
