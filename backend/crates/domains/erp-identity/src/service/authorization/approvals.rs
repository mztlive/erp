//! 审批管理按真实冻结类型核验同角色管理资格和完整业务来源。
use std::collections::BTreeSet;

use erp_permissions::{
    approval_admin, approval_instance, internal, inventory, payable, policy_permission, purchase_order,
    purchase_payment_request, receivable, sales_order, work_item,
};
use persistence_core::Executor;

use super::resolve::Resolution;
use super::targets::{Target, TargetFacts};
use super::{AuthorizationService, rules};
use crate::entity::access_control::ResolvedScope;
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{
    AccessContext, AccessRule, FactKind, FactValue, ObjectFacts, RelationKey, ResourceTargets,
};
use crate::entity::fingerprint::Fingerprint;
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::{AccessProof, ApprovalSource};
use crate::{Error, Permission, QualifiedPermissions, Result};

pub(super) fn supports(permission: &Permission) -> bool {
    [
        internal::APPROVAL_MANAGEMENT_READ,
        internal::APPROVAL_INSTANCE_READ,
        internal::APPROVAL_RECOVER,
        approval_instance::CANCEL_BLOCKED,
        approval_instance::UPGRADE_BINDING,
        inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
    ]
    .contains(permission)
}

/// 初次资格取证同时覆盖全部潜在类型和来源；实际类型只能由持久化事实确定。
pub(super) fn qualifications(permission: &Permission) -> Vec<Permission> {
    if !supports(permission) {
        return vec![];
    }
    let mut permissions = vec![approval_instance::INSTANCE_LIST];
    if permission == &internal::APPROVAL_INSTANCE_READ {
        permissions.push(approval_instance::SUBMIT_DECISION);
    }
    for (_, definition, runtime) in approval_admin::TYPES {
        permissions.push(definition.clone());
        permissions.push(runtime.clone());
    }
    permissions.push(policy_permission::FINANCE_LEDGER_READ);
    if permission == &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL {
        permissions.push(inventory::STOCK_ADJUSTMENT_DETAIL);
    }
    permissions.sort();
    permissions.dedup();
    permissions
}

pub(super) fn source_permissions() -> Vec<Permission> {
    vec![
        sales_order::DETAIL,
        purchase_order::DETAIL,
        inventory::STOCK_ADJUSTMENT_DETAIL,
        purchase_payment_request::DETAIL,
        receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
        receivable::CUSTOMER_RECEIPT_DETAIL,
        receivable::ACCOUNT_DETAIL,
        payable::SUPPLIER_PAYMENT_DETAIL,
        payable::ACCOUNT_DETAIL,
    ]
}

fn requirement<'a>(entry: &Permission, object: &'a ObjectFacts) -> Result<(Permission, &'a ObjectFacts)> {
    let subject_type = object.subject_type.get()?;
    let (_, definition, runtime) = approval_admin::TYPES
        .iter()
        .find(|(kind, _, _)| *kind == subject_type)
        .ok_or_else(|| Error::ValidationError("审批管理的冻结主体类型未登记".into()))?;
    let admin = if entry == &approval_instance::UPGRADE_BINDING { definition } else { runtime };

    let related = object
        .relations
        .get(&RelationKey::parse("subject_source")?)
        .ok_or_else(|| Error::ValidationError("审批管理缺少真实业务来源".into()))?;
    related.validate()?;
    let [source] = related.objects.as_slice() else {
        return Err(Error::ValidationError("审批管理必须加载唯一完整来源".into()));
    };
    if related.expected_ids.len() != 1 {
        return Err(Error::ValidationError("审批管理的完整来源集合不符".into()));
    }
    if !allows_source(subject_type, &source.resource) {
        return Err(Error::ValidationError("审批冻结类型与真实授权来源不匹配".into()));
    }
    Ok((admin.clone(), source))
}

pub(super) fn allows_source(subject_type: &str, resource: &str) -> bool {
    match subject_type {
        "sales_order" | "voucher_sales_order" | "sales_change_order" => resource == "sales_order",
        "purchase_order" | "purchase_change_order" => resource == "purchase_order",
        "stock_adjustment" => resource == "stock_adjustment",
        "sales_invoice_request" => resource == "sales_invoice_request",
        "purchase_payment_request" => resource == "purchase_payment_request",
        "customer_receipt" | "receipt_reversal" => resource == "customer_receipt",
        "payment_reversal" => resource == "supplier_payment",
        "customer_refund" => matches!(resource, "customer_receipt" | "receivable_account"),
        "supplier_refund" => matches!(resource, "supplier_payment" | "payable_account"),
        _ => false,
    }
}

pub(super) fn source_permission(resource: &str) -> Result<Permission> {
    source_permissions()
        .into_iter()
        .find(|permission| permission.resource() == resource)
        .ok_or_else(|| Error::ValidationError("审批管理的真实来源未接入授权".into()))
}

pub(super) fn candidate_source(candidate: &ObjectFacts) -> Result<(RelationKey, String)> {
    let kind = candidate.subject_type.get()?;
    let resource = match &candidate.subject_source_resource {
        FactValue::Known(resource) => resource.as_str(),
        FactValue::Missing => match kind.as_str() {
            "sales_order" | "voucher_sales_order" | "sales_change_order" => "sales_order",
            "purchase_order" | "purchase_change_order" => "purchase_order",
            "stock_adjustment" => "stock_adjustment",
            "sales_invoice_request" => "sales_invoice_request",
            "purchase_payment_request" => "purchase_payment_request",
            "customer_receipt" | "receipt_reversal" => "customer_receipt",
            "payment_reversal" => "supplier_payment",
            _ => return Err(Error::ValidationError("审批升级候选缺少真实来源资源身份".into())),
        },
    };
    if !allows_source(kind, resource) {
        return Err(Error::ValidationError("审批升级的真实来源与主体类型错配".into()));
    }

    let key = RelationKey::parse("subject_source")?;
    let related = candidate
        .relations
        .get(&key)
        .ok_or_else(|| Error::ValidationError("审批升级缺少完整真实来源引用".into()))?;
    if related.expected_ids.len() != 1 || !related.objects.is_empty() {
        return Err(Error::ValidationError("审批升级候选只能提供唯一真实来源引用".into()));
    }
    Ok((key, resource.into()))
}

pub(super) fn management_permissions(entry: &Permission, admin: &Permission) -> Vec<Permission> {
    let entry = if entry == &internal::APPROVAL_RECOVER {
        approval_instance::RESUME_CURRENT_APPROVER
    } else if entry == &internal::APPROVAL_MANAGEMENT_READ || entry == &internal::APPROVAL_INSTANCE_READ {
        approval_instance::INSTANCE_LIST
    } else {
        entry.clone()
    };
    vec![entry, approval_instance::INSTANCE_LIST, admin.clone()]
}

fn ensure_admin(entry: &Permission, admin: Permission, qualified: &QualifiedPermissions) -> Result<()> {
    let required = management_permissions(entry, &admin);
    if qualified.same_role(&required) {
        Ok(())
    } else {
        Err(Error::Forbidden("当前账号不具备该主体类型的同角色完整审批管理资格".into()))
    }
}

impl AuthorizationService {
    /// 使用已解析的账号资格检查实际审批源，不开启第二授权阶段。
    pub(super) async fn approval_sources(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccessProof>> {
        if !supports(&proof.context.permission) {
            return Ok(vec![]);
        }
        if proof.context.permission == inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL {
            return self.stock_adjustment_cancel_sources(proof, facts, executor).await;
        }

        let mut sources = Vec::new();
        for object in facts.roots() {
            if proof.context.permission == internal::APPROVAL_INSTANCE_READ
                && ordinary_readable(object, proof.context.actor.id(), &proof.qualified)?
            {
                continue;
            }

            let expanded = if proof.context.permission == internal::APPROVAL_INSTANCE_READ {
                Some(self.management_facts(proof, object, executor).await?)
            } else {
                None
            };
            let source_facts = expanded.as_ref().unwrap_or(facts);
            let object = expanded.as_ref().and_then(|facts| facts.roots().next()).unwrap_or(object);
            sources.push(self.approval_source(proof, object, source_facts, executor).await?);
        }
        Ok(sources)
    }

    /// 真实类型管理资格、完整来源、显式授权及全部到期边界在同一读取阶段核验。
    async fn approval_source(
        &self,
        proof: &AccessProof,
        object: &ObjectFacts,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let (admin, source) = requirement(&proof.context.permission, object)?;
        ensure_admin(&proof.context.permission, admin.clone(), &proof.qualified)?;
        let permission = source_permission(&source.resource)?;
        let mut required = management_permissions(&proof.context.permission, &admin);
        if matches!(source.resource.as_str(), "customer_receipt" | "supplier_payment") {
            required.push(policy_permission::FINANCE_LEDGER_READ);
        }

        let mut source_proof = self
            .resolve_approval_source(&proof.context, &permission, &required, &proof.qualified, executor)
            .await?;

        let grants = self.target_grants(&source_proof.context, &source_proof.rule, facts, executor).await?;
        let earliest = facts.flattened()?.iter().filter_map(|fact| fact.valid_until).min();
        source_proof.valid_until = source_proof.valid_until.into_iter().chain(earliest).min();
        if !source_proof.rule.allows_object(proof.context.actor.id(), source, &grants)? {
            return Err(Error::Forbidden("审批真实来源不在当前可读取范围内".into()));
        }
        source_proof.ensure_current(executor)?;
        Ok(source_proof)
    }

    /// 普通读取未命中时在原快照按需加载管理来源，不给普通读追加当前来源要求。
    async fn management_facts(
        &self,
        proof: &AccessProof,
        object: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<TargetFacts> {
        if !proof.boundary.is_snapshot() {
            return Err(Error::ValidationError("审批管理读取必须使用快照事务".into()));
        }

        let context =
            AccessContext { permission: internal::APPROVAL_MANAGEMENT_READ, ..proof.context.clone() };
        let targets = ResourceTargets::new(&[object.id.as_str()])?;
        let expanded_facts =
            TargetFacts::load(&self.facts, &context, &AccessRule::Allow, Target::Records(&targets), executor)
                .await?;

        let expanded = expanded_facts
            .roots()
            .next()
            .ok_or_else(|| Error::ValidationError("审批管理实例事实缺失".into()))?;
        if expanded.version != object.version || expanded.subject_type != object.subject_type {
            return Err(Error::ConflictError("审批实例管理取证期间发生变化".into()));
        }
        Ok(expanded_facts)
    }
}

/// 固定普通读取关系；当前职责同时要求同角色办理资格，历史只使用实际决定事实。
fn ordinary_readable(object: &ObjectFacts, actor: &str, qualified: &QualifiedPermissions) -> Result<bool> {
    let owner = object.owner_id.get()?;
    let assigned = object.assignee_id.get()?;
    let participants = object.participants.get()?;
    Ok(owner.as_deref() == Some(actor)
        || participants.contains(actor)
        || (assigned.as_deref() == Some(actor)
            && qualified.same_role(&[approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION])))
}

impl AuthorizationService {
    /// 以完整审批管理角色解析实际业务来源，不追加来源菜单资格。
    pub(super) async fn resolve_approval_source(
        &self,
        entry: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        if !qualified.same_role(required) {
            return Err(Error::Forbidden("审批管理的真实来源缺少完整同角色资格".into()));
        }

        let context = AccessContext { resource: source.resource().into(), model: None, ..entry.clone() };
        let mut resolution = Resolution { versions: vec![qualified.version().into()], ..Default::default() };
        let rule = self
            .approval_source_condition(&context, source, required, qualified, &mut resolution, executor)
            .await?;

        let mut resources = BTreeSet::new();
        rule.grant_resources(&mut resources);
        for resource in resources {
            resolution.versions.push(format!(
                "{}:{}",
                resource.code(),
                self.facilities.grant_revision(&context.actor, resource, executor).await?
            ));
        }

        let source = ApprovalSource { source: source.clone(), required: required.to_vec() };
        self.approval_source_proof(context, source, qualified, rule, resolution, executor)
    }

    /// 固定管理上下文和当前事务边界签发本次专有来源证明。
    fn approval_source_proof(
        &self,
        context: AccessContext,
        source: ApprovalSource,
        qualified: &QualifiedPermissions,
        rule: AccessRule,
        resolution: Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let mut version = Fingerprint::new("approval-source-v1");
        version.unordered(&resolution.versions, |hash, value| hash.text(value));
        version.rule(&rule);

        let proof = AccessProof {
            context,
            qualified: qualified.clone(),
            approval_source: Some(source),
            task: false,
            rule,
            whole_rule: None,
            management_rule: None,
            has_role_scope: resolution.has_role_scope,
            version: version.finish(),
            valid_until: resolution.valid_until,
            policy_version: qualified.policy_revision(),
            organization_version: resolution.organization_version,
            boundary: executor
                .boundary()
                .ok_or_else(|| Error::ValidationError("审批来源缺少活动读取边界".into()))?,
            service: self.token.clone(),
        };
        proof.ensure_current(executor)?;
        Ok(proof)
    }

    pub(super) async fn approval_source_condition(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let requests = if scope_spec::kind(source)? == ActionKind::Source {
            rules::sources::scope_requests(source)?
        } else {
            vec![(source.clone(), source.clone())]
        };
        let mut scopes = Vec::new();
        for (_, scope_source) in requests {
            if scopes.iter().any(|(key, _)| key == &scope_source) {
                continue;
            }
            let origin = scope_spec::runtime_source(scope_spec::require_scope(&scope_source)?)?.permission();
            let scope = self.facilities.scope(context, origin, required, qualified, executor).await?;
            resolution.include(&scope);
            scopes.push((scope_source, scope.scope.clone()));
        }

        if context.permission == work_item::DETAIL && source == &purchase_order::DETAIL {
            let scope =
                &scopes.first().ok_or_else(|| Error::ValidationError("任务采购来源范围未加载".into()))?.1;
            return rules::data_condition(source, Some(scope));
        }
        scoped_source_condition(source, &scopes)
    }
}

/// 对象与列表使用相同的管理来源范围、记录读取补充及完整性条件。
pub(super) fn scoped_source_condition(
    source: &Permission,
    scopes: &[(Permission, ResolvedScope)],
) -> Result<AccessRule> {
    if scope_spec::kind(source)? == ActionKind::Source {
        return rules::sources::management_condition(source, scopes);
    }
    let scope = &scopes.first().ok_or_else(|| Error::ValidationError("审批来源范围未加载".into()))?.1;
    if let Some(resource) = match source.resource() {
        "sales_order" => Some(ResourceKind::SalesOrder),
        "purchase_order" => Some(ResourceKind::PurchaseOrder),
        _ => None,
    } {
        return Ok(AccessRule::any(vec![
            AccessRule::scope(scope, false),
            AccessRule::RecordReadGrant(resource),
        ])
        .requiring(&[FactKind::Owner, FactKind::Organization].into()));
    }
    rules::data_condition(source, Some(scope))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::authorization::{FactValue, RelatedObjects};

    fn instance(subject_type: &str, source: &str) -> ObjectFacts {
        let child = ObjectFacts {
            resource: source.into(),
            id: "source".into(),
            version: "1".into(),
            ..Default::default()
        };
        ObjectFacts {
            resource: "approval_instance".into(),
            id: "instance".into(),
            version: "1".into(),
            subject_type: FactValue::Known(subject_type.into()),
            relations: [(
                RelationKey::parse("subject_source").unwrap(),
                RelatedObjects { expected_ids: ["source".into()].into(), objects: vec![child] },
            )]
            .into(),
            ..Default::default()
        }
    }

    #[test]
    fn management_requires_the_actual_type_admin_in_the_same_enabled_role() {
        let entry = approval_instance::CANCEL_BLOCKED;
        let object = instance("sales_order", "sales_order");
        let (admin, _) = requirement(&entry, &object).unwrap();
        let required = [entry.clone(), approval_instance::INSTANCE_LIST, admin.clone()];
        assert!(
            ensure_admin(&entry, admin.clone(), &QualifiedPermissions::test_grants(&[("role", &required)]))
                .is_ok()
        );

        let split =
            QualifiedPermissions::test_grants(&[("entry", &required[..2]), ("admin", &required[2..])]);
        assert!(matches!(ensure_admin(&entry, admin.clone(), &split), Err(Error::Forbidden(_))));

        let wrong_type =
            [entry.clone(), approval_instance::INSTANCE_LIST, approval_admin::PURCHASE_ORDER_RUNTIME];
        assert!(
            ensure_admin(&entry, admin, &QualifiedPermissions::test_grants(&[("wrong", &wrong_type)]))
                .is_err()
        );
    }

    #[test]
    fn binding_upgrade_uses_definition_admin_and_rejects_missing_or_wrong_sources() {
        let entry = approval_instance::UPGRADE_BINDING;
        let mut object = instance("receipt_reversal", "customer_receipt");
        assert_eq!(requirement(&entry, &object).unwrap().0, approval_admin::RECEIPT_REVERSAL_DEFINITION);
        object.subject_type = FactValue::Known("purchase_order".into());
        assert!(requirement(&entry, &object).is_err());
        object.subject_type = FactValue::Missing;
        assert!(requirement(&entry, &object).is_err());

        object = instance("sales_order", "sales_order");
        object.relations.values_mut().next().unwrap().objects.clear();
        assert!(requirement(&entry, &object).is_err());
    }
}
