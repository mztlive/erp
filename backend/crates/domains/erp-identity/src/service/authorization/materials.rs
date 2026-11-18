//! 门户材料按固定申请动作检查真实供应商和既有供给的独立范围。
use erp_permissions::{Permission, supplier, supplier_offering};
use persistence_core::Executor;

use super::resolve::Resolution;
use super::{AuthorizationService, rules};
use crate::entity::authorization::{AccessContext, AccessRule, RelationKey, RelationMatch};
use crate::{QualifiedPermissions, Result};

// 识别使用已冻结真实来源关系的固定门户材料操作，不为其他操作启用这条来源规则。
pub(super) fn supports(permission: &Permission) -> bool {
    permission.resource() == "supplier_portal_material"
}

// 为门户材料逐个声明独立来源的合格角色范围。创建资格仍按既有功能权限检查，不套用读取范围。
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

// 为门户材料登记校验构造完整来源条件；所需事实维度未接入时拒绝，不视作无数据限制。
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
    // 在材料读取时恢复各真实来源的独立范围，门户任务资格不能替代来源角色的资格。
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
