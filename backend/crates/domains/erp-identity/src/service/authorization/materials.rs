//! 门户材料按固定申请动作检查真实供应商和既有供给的独立范围。
use erp_permissions::{Permission, supplier, supplier_offering};
use persistence_core::Executor;

use super::resolve::Resolution;
use super::{AuthorizationService, rules};
use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::{AccessContext, AccessRule, RelationKey, RelationMatch};
use crate::{QualifiedPermissions, Result};

/// 识别按固定真实供应商和供给关系授权的门户材料资源，不为其他资源启用本规则。
pub(super) fn supports(permission: &Permission) -> bool {
    permission.resource() == "supplier_portal_material"
}

// 按材料读审动作列出须独立取证的供应商及既有供给；创建资格仍按既有功能权限，不套用读取范围。
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

/// 启动时展开门户材料全部固定来源能力，必需事实维度缺失时拒绝绑定，不去掉限制。
pub(super) fn binding_condition(permission: &Permission) -> Result<AccessRule> {
    sources(permission)
        .into_iter()
        .map(|(relation, source)| {
            let scope = rules::capability_scope(&source)?;
            source_condition(relation, &source, &scope)
        })
        .collect::<Result<Vec<_>>>()
        .map(AccessRule::all)
}

// 将单个材料来源的范围编成关联规则，要求引用非空且所有真实来源均可访问。
fn source_condition(relation: &str, source: &Permission, scope: &ResolvedScope) -> Result<AccessRule> {
    Ok(AccessRule::Related {
        relation: RelationKey::parse(relation)?,
        resource: source.resource().into(),
        mode: RelationMatch::AllNonEmpty,
        rule: Box::new(rules::data_condition(source, Some(scope))?),
    })
}

impl AuthorizationService {
    /// 读取或审核材料时分别解析真实来源的独立范围，并汇入本次版本与期限；门户任务资格不能替代来源资格。
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
            rules.push(source_condition(relation, &permission, &resolution.include(scope))?);
        }
        Ok(AccessRule::all(rules))
    }
}
