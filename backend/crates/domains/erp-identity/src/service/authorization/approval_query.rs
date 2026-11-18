//! 管理列表按冻结类型和唯一真实来源编译完整对象条件。
use erp_permissions::{approval_admin, internal, policy_permission};
use persistence_core::Executor;

use super::approvals::{allows_source, management_permissions, scoped_source_condition, source_permissions};
use super::resolve::Resolution;
use super::{AuthorizationService, rules};
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{AccessContext, AccessRule, RelationKey, RelationMatch};
use crate::{Permission, QualifiedPermissions, Result};

/// 关系名只表达固定的冻结主体类型与来源资源，不携带请求状态。
fn relation(subject: &str, source: &Permission, rule: AccessRule) -> Result<AccessRule> {
    Ok(AccessRule::Related {
        relation: RelationKey::parse(format!("management_{subject}_{}", source.resource()))?,
        resource: source.resource().into(),
        mode: RelationMatch::ExactlyOne,
        rule: Box::new(rule),
    })
}

/// 启动时校验所有固定分区与嵌套来源能力，不受账号当前资格影响。
pub(super) fn query_binding_condition() -> Result<AccessRule> {
    let mut partitions = Vec::new();
    for (subject, _, _) in approval_admin::TYPES {
        for source in source_permissions().into_iter().filter(|p| allows_source(subject, p.resource())) {
            let requests = if scope_spec::kind(&source)? == ActionKind::Source {
                rules::sources::scope_requests(&source)?
            } else {
                vec![(source.clone(), source.clone())]
            };
            let scopes = requests
                .into_iter()
                .map(|(_, source)| Ok((source.clone(), rules::capability_scope(&source)?)))
                .collect::<Result<Vec<_>>>()?;
            partitions.push(relation(subject, &source, scoped_source_condition(&source, &scopes)?)?);
        }
    }
    Ok(AccessRule::any(partitions))
}

impl AuthorizationService {
    /// 所有分区共用一次身份取证；每类管理资格独立约束其来源范围。
    pub(super) async fn approval_query_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let mut partitions = Vec::new();
        for (subject, _, admin) in approval_admin::TYPES {
            let required = management_permissions(&internal::APPROVAL_MANAGEMENT_READ, admin);
            for source in source_permissions().into_iter().filter(|p| allows_source(subject, p.resource())) {
                let mut required = required.clone();
                if matches!(source.resource(), "customer_receipt" | "supplier_payment") {
                    required.push(policy_permission::FINANCE_LEDGER_READ);
                }
                if !qualified.same_role(&required) {
                    continue;
                }

                let source_context = AccessContext { resource: source.resource().into(), ..context.clone() };
                let rule = self
                    .approval_source_condition(
                        &source_context,
                        &source,
                        &required,
                        qualified,
                        resolution,
                        executor,
                    )
                    .await?;
                partitions.push(relation(subject, &source, rule)?);
            }
        }
        Ok(AccessRule::any(partitions))
    }
}
