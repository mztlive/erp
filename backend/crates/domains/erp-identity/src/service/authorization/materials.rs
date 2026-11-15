//! 门户材料按固定申请动作检查真实供应商和既有供给的独立范围。
use erp_permissions::{Permission, supplier, supplier_offering};
use persistence_core::Executor;

use super::resolve::Resolution;
use super::{AuthorizationService, rules};
use crate::entity::authorization::{AccessContext, AccessRule, RelationKey, RelationMatch};
use crate::{QualifiedPermissions, Result};

/// 门户材料的固定操作均使用已冻结的真实来源关系。
pub(super) fn supports(permission: &Permission) -> bool {
    permission.resource() == "supplier_portal_material"
}

/// 每个来源的范围使用该来源独立合格角色；创建资格只按既有功能要求取证。
fn sources(permission: &Permission) -> Vec<(&'static str, Permission)> {
    let supplier =
        if permission.action() == "review_cooperation" { supplier::PROFILE_UPDATE } else { supplier::DETAIL };
    let mut sources = vec![("supplier", supplier)];
    match permission.action() {
        "read_offering_update" => sources.push(("offering", supplier_offering::LIST)),
        "review_offering_update" => sources.push(("offering", supplier_offering::REVISE)),
        _ => {},
    }
    sources
}

/// 启动绑定验证沿同一固定来源清单，不能把缺少维度的材料事实当成无数据条件。
pub(super) fn binding_condition(permission: &Permission) -> Result<AccessRule> {
    let rules = sources(permission)
        .into_iter()
        .map(|(relation, source)| {
            let scope = rules::capability_scope(&source)?;
            Ok(AccessRule::Related {
                relation: RelationKey::parse(relation)?,
                resource: source.resource().into(),
                mode: RelationMatch::AllNonEmpty,
                rule: Box::new(rules::data_condition(&source, Some(&scope))?),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AccessRule::all(rules))
}

impl AuthorizationService {
    /// 恢复材料原有的独立来源范围；不把门户任务角色拼成来源角色。
    pub(super) async fn material_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let mut rules = Vec::new();
        for (relation, permission) in sources(&context.permission) {
            let source = AccessContext {
                permission: permission.clone(),
                resource: permission.resource().into(),
                ..context.clone()
            };
            let scope = self.resolve_scope(&source, &permission, qualified, executor).await?;
            resolution.include(&scope);
            rules.push(AccessRule::Related {
                relation: RelationKey::parse(relation)?,
                resource: permission.resource().into(),
                mode: RelationMatch::AllNonEmpty,
                rule: Box::new(rules::data_condition(&permission, Some(&scope.scope))?),
            });
        }
        Ok(AccessRule::all(rules))
    }
}
