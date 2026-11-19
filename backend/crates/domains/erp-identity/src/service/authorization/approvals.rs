//! 审批管理按真实冻结类型核验同角色管理资格和完整业务来源。
use std::collections::BTreeSet;

use erp_permissions::{
    approval_admin, approval_instance, internal, inventory, payable, policy_permission, purchase_order,
    purchase_payment_request, receivable, resources, sales_order, work_item,
};
use persistence_core::Executor;

use super::resolve::Resolution;
use super::targets::TargetFacts;
use super::{AuthorizationService, rules};
use crate::entity::access_control::ResolvedScope;
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{
    AccessContext, AccessRule, AccessTarget, FactKind, FactValue, ObjectFacts, RelationKey,
};
use crate::entity::fingerprint::Fingerprint;
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::{
    AccessProof, AccessValidity, Replay, ResolvedAccess, ResolvedRules, ScopeMetadata,
};
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

// 对象加载前一次取得各可能审批类型及来源所需资格；实际单据类型仍从持久化主体确定。
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
    permissions.extend(MANAGEMENT_SOURCES.iter().filter_map(|source| source.additional.clone()));
    if permission == &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL {
        permissions.push(inventory::STOCK_ADJUSTMENT_DETAIL);
    }
    permissions.sort();
    permissions.dedup();
    permissions
}

pub(super) struct ManagementSource {
    pub(super) permission: Permission,
    subjects: &'static [&'static str],
    additional: Option<Permission>,
}

impl ManagementSource {
    const fn scoped(permission: Permission, subjects: &'static [&'static str]) -> Self {
        Self { permission, subjects, additional: None }
    }

    const fn whole_ledger(permission: Permission, subjects: &'static [&'static str]) -> Self {
        Self { permission, subjects, additional: Some(policy_permission::FINANCE_LEDGER_READ) }
    }

    pub(super) fn add_qualification(&self, required: &mut Vec<Permission>) {
        required.extend(self.additional.clone());
    }
}

// 分区先按冻结类型登记顺序，再按同一类型的真实来源顺序展开。
static MANAGEMENT_SOURCES: &[ManagementSource] = &[
    ManagementSource::scoped(
        sales_order::DETAIL,
        &[resources::SALES_ORDER, resources::VOUCHER_SALES_ORDER, resources::SALES_CHANGE_ORDER],
    ),
    ManagementSource::scoped(
        purchase_order::DETAIL,
        &[resources::PURCHASE_ORDER, resources::PURCHASE_CHANGE_ORDER],
    ),
    ManagementSource::scoped(inventory::STOCK_ADJUSTMENT_DETAIL, &[resources::STOCK_ADJUSTMENT]),
    ManagementSource::scoped(purchase_payment_request::DETAIL, &[resources::PURCHASE_PAYMENT_REQUEST]),
    ManagementSource::scoped(
        receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
        &[resources::SALES_INVOICE_REQUEST],
    ),
    ManagementSource::whole_ledger(
        receivable::CUSTOMER_RECEIPT_DETAIL,
        &[resources::CUSTOMER_RECEIPT, resources::RECEIPT_REVERSAL, resources::CUSTOMER_REFUND],
    ),
    ManagementSource::scoped(receivable::ACCOUNT_DETAIL, &[resources::CUSTOMER_REFUND]),
    ManagementSource::whole_ledger(
        payable::SUPPLIER_PAYMENT_DETAIL,
        &[resources::PAYMENT_REVERSAL, resources::SUPPLIER_REFUND],
    ),
    ManagementSource::scoped(payable::ACCOUNT_DETAIL, &[resources::SUPPLIER_REFUND]),
];

fn subject_sources(subject: &str) -> impl Iterator<Item = &'static ManagementSource> {
    MANAGEMENT_SOURCES.iter().filter(move |source| source.subjects.contains(&subject))
}

fn subject_source(subject: &str, resource: &str) -> Option<&'static ManagementSource> {
    subject_sources(subject).find(|source| source.permission.resource() == resource)
}

pub(super) fn management_partitions()
-> impl Iterator<Item = (&'static str, &'static Permission, &'static ManagementSource)> {
    approval_admin::TYPES.iter().flat_map(|(subject, _, admin)| {
        let subject = *subject;
        subject_sources(subject).map(move |source| (subject, admin, source))
    })
}

fn requirement<'a>(
    entry: &Permission,
    object: &'a ObjectFacts,
) -> Result<(&'static Permission, &'static ManagementSource, &'a ObjectFacts)> {
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
    let policy = subject_source(subject_type, &source.resource)
        .ok_or_else(|| Error::ValidationError("审批冻结类型与真实授权来源不匹配".into()))?;
    Ok((admin, policy, source))
}

pub(super) fn source_permission(resource: &str) -> Result<Permission> {
    MANAGEMENT_SOURCES
        .iter()
        .find(|source| source.permission.resource() == resource)
        .map(|source| source.permission.clone())
        .ok_or_else(|| Error::ValidationError("审批管理的真实来源未接入授权".into()))
}

pub(super) fn candidate_source(candidate: &ObjectFacts) -> Result<(RelationKey, String)> {
    let kind = candidate.subject_type.get()?;
    let source = match &candidate.subject_source_resource {
        FactValue::Known(resource) => subject_source(kind, resource)
            .ok_or_else(|| Error::ValidationError("审批升级的真实来源与主体类型错配".into()))?,
        FactValue::Missing => {
            let mut sources = subject_sources(kind);
            let (Some(source), None) = (sources.next(), sources.next()) else {
                return Err(Error::ValidationError("审批升级候选缺少真实来源资源身份".into()));
            };
            source
        },
    };

    let key = RelationKey::parse("subject_source")?;
    let related = candidate
        .relations
        .get(&key)
        .ok_or_else(|| Error::ValidationError("审批升级缺少完整真实来源引用".into()))?;
    if related.expected_ids.len() != 1 || !related.objects.is_empty() {
        return Err(Error::ValidationError("审批升级候选只能提供唯一真实来源引用".into()));
    }
    Ok((key, source.permission.resource().into()))
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

fn ensure_admin(required: &[Permission], qualified: &QualifiedPermissions) -> Result<()> {
    if qualified.same_role(required) {
        Ok(())
    } else {
        Err(Error::Forbidden("当前账号不具备该主体类型的同角色完整审批管理资格".into()))
    }
}

impl AuthorizationService {
    // 审批对象取证后，以同次已解析身份检查真实业务来源，不开始新的读取阶段或换用执行器。
    pub(super) async fn approval_sources(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccessProof>> {
        if !supports(&proof.context.permission) {
            return Ok(vec![]);
        }
        if proof.context.permission == inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL {
            return self.stock_adjustment_cancel_sources(proof, qualified, facts, executor).await;
        }

        let mut sources = Vec::new();
        for object in facts.roots() {
            if proof.context.permission == internal::APPROVAL_INSTANCE_READ
                && ordinary_readable(object, proof.context.actor.id(), qualified)?
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
            sources.push(self.approval_source(proof, qualified, object, source_facts, executor).await?);
        }
        Ok(sources)
    }

    // 在原读取阶段校验真实单据类型的管理资格、完整来源及显式记录授权，保留全部最早到期边界供交付复验。
    async fn approval_source(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        object: &ObjectFacts,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let entry = &proof.context.permission;
        let (admin, policy, source) = requirement(entry, object)?;
        let mut required = management_permissions(entry, admin);
        ensure_admin(&required, qualified)?;
        policy.add_qualification(&mut required);
        let mut source_proof = self
            .resolve_approval_source(&proof.context, &policy.permission, &required, qualified, executor)
            .await?;

        let grants = self
            .target_grants(&source_proof.context, &source_proof.resolved.rules.root, facts, executor)
            .await?;
        let earliest = facts.flattened()?.iter().filter_map(|fact| fact.valid_until).min();
        source_proof.validity.valid_until =
            source_proof.validity.valid_until.into_iter().chain(earliest).min();
        if !source_proof.resolved.rules.root.allows_object(proof.context.actor.id(), source, &grants)? {
            return Err(Error::Forbidden("审批真实来源不在当前可读取范围内".into()));
        }
        source_proof.validity.ensure_current(executor)?;
        Ok(source_proof)
    }

    // 普通审批读取未命中时再沿原快照加载管理分支需要的业务来源，不给正常本人读取追加当前来源要求。
    async fn management_facts(
        &self,
        proof: &AccessProof,
        object: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<TargetFacts> {
        if !proof.validity.boundary.is_snapshot() {
            return Err(Error::ValidationError("审批管理读取必须使用快照事务".into()));
        }

        let context =
            AccessContext { permission: internal::APPROVAL_MANAGEMENT_READ, ..proof.context.clone() };
        let target = AccessTarget::record(&object.id)?;
        let expanded_facts =
            TargetFacts::load(&self.facts, &context, &AccessRule::Allow, target, executor).await?;

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

// 判断固定普通审批读取分支：现职责须满足同角色办理资格，历史参与只接受真实已决定记录。
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
    // 用满足审批管理完整资格的角色解析真实来源范围，不额外要求该来源的菜单权限。
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

        let rules = ResolvedRules { root: rule, whole: None, management: None };
        resolution.versions.extend(self.grant_versions(&context.actor, &rules, executor).await?);
        let mut version = Fingerprint::new("approval-source-v1");
        version.unordered(&resolution.versions, |hash, value| hash.text(value));
        version.rule(&rules.root);

        let proof = AccessProof {
            context,
            resolved: ResolvedAccess {
                rules,
                replay: Replay::ApprovalSource { source: source.clone(), required: required.to_vec() },
                version: version.finish(),
            },
            validity: AccessValidity {
                valid_until: resolution.valid_until,
                boundary: executor
                    .boundary()
                    .ok_or_else(|| Error::ValidationError("审批来源缺少活动读取边界".into()))?,
                service: self.token.clone(),
            },
            scope: ScopeMetadata {
                has_role_scope: resolution.has_role_scope,
                organization_version: resolution.organization_version,
            },
        };
        proof.validity.ensure_current(executor)?;
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
        let mut scopes = Vec::new();
        for scope_source in management_scope_requests(source)? {
            let origin = scope_spec::runtime_source(scope_spec::require_scope(&scope_source)?)?.permission();
            let scope = self.facilities.scope(context, origin, required, qualified, executor).await?;
            resolution.include(&scope);
            scopes.push((scope_source, scope.scope));
        }

        if context.permission == work_item::DETAIL && source == &purchase_order::DETAIL {
            let scope =
                &scopes.first().ok_or_else(|| Error::ValidationError("任务采购来源范围未加载".into()))?.1;
            return rules::data_condition(source, Some(scope));
        }
        scoped_source_condition(source, &scopes)
    }
}

// 管理资格统一解释来源范围；同一范围权限只解析一次，保留其首次出现顺序。
pub(super) fn management_scope_requests(source: &Permission) -> Result<Vec<Permission>> {
    if scope_spec::kind(source)? != ActionKind::Source {
        return Ok(vec![source.clone()]);
    }

    let mut seen = BTreeSet::new();
    Ok(rules::sources::scope_requests(source)?
        .into_iter()
        .map(|(_, source)| source)
        .filter(|source| seen.insert(source.clone()))
        .collect())
}

// 为审批管理构建对象与列表共用的来源范围条件，同时保留读取补充和完整性要求。
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
        let (admin, _, _) = requirement(&entry, &object).unwrap();
        let required = management_permissions(&entry, admin);
        assert!(ensure_admin(&required, &QualifiedPermissions::test_grants(&[("role", &required)])).is_ok());

        let split =
            QualifiedPermissions::test_grants(&[("entry", &required[..2]), ("admin", &required[2..])]);
        assert!(matches!(ensure_admin(&required, &split), Err(Error::Forbidden(_))));

        let wrong_type = [entry, approval_instance::INSTANCE_LIST, approval_admin::PURCHASE_ORDER_RUNTIME];
        assert!(
            ensure_admin(&required, &QualifiedPermissions::test_grants(&[("wrong", &wrong_type)])).is_err()
        );
    }

    #[test]
    fn upgrade_candidate_only_defaults_a_unique_legal_source() {
        let mut candidate = instance("receipt_reversal", "customer_receipt");
        candidate.relations.values_mut().next().unwrap().objects.clear();
        let (key, resource) = candidate_source(&candidate).unwrap();
        assert_eq!(key.as_str(), "subject_source");
        assert_eq!(resource, "customer_receipt");

        for (kind, legal, wrong) in [
            ("customer_refund", ["customer_receipt", "receivable_account"], "payable_account"),
            ("supplier_refund", ["supplier_payment", "payable_account"], "receivable_account"),
        ] {
            candidate.subject_type = FactValue::Known(kind.into());
            candidate.subject_source_resource = FactValue::Missing;
            assert!(matches!(
                candidate_source(&candidate),
                Err(Error::ValidationError(message)) if message == "审批升级候选缺少真实来源资源身份"
            ));
            for source in legal {
                candidate.subject_source_resource = FactValue::Known(source.into());
                assert_eq!(candidate_source(&candidate).unwrap().1, source);
            }
            candidate.subject_source_resource = FactValue::Known(wrong.into());
            assert!(matches!(
                candidate_source(&candidate),
                Err(Error::ValidationError(message)) if message == "审批升级的真实来源与主体类型错配"
            ));
        }
    }

    #[test]
    fn binding_upgrade_uses_definition_admin_and_rejects_missing_or_wrong_sources() {
        let entry = approval_instance::UPGRADE_BINDING;
        let mut object = instance("receipt_reversal", "customer_receipt");
        assert_eq!(requirement(&entry, &object).unwrap().0, &approval_admin::RECEIPT_REVERSAL_DEFINITION);
        object.subject_type = FactValue::Known("purchase_order".into());
        assert!(requirement(&entry, &object).is_err());
        object.subject_type = FactValue::Missing;
        assert!(requirement(&entry, &object).is_err());

        object = instance("sales_order", "sales_order");
        object.relations.values_mut().next().unwrap().objects.clear();
        assert!(requirement(&entry, &object).is_err());
    }
}
