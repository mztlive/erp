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

/// 判断固定入口是否须执行审批类型及来源检查，含库存调整撤回的专用分支。
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

/// 在加载审批对象前一次预取候选类型和来源所需资格；实际类型由冻结主体事实决定。
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

/// 登记冻结审批类型可使用的真实来源、取范围入口及整账附加资格。
pub(super) struct ManagementSource {
    /// 该来源的范围动作，同时标识真实业务资源，并非附加菜单门槛。
    pub(super) permission: Permission,
    // 可关联此来源的冻结审批主体类型。
    subjects: &'static [&'static str],
    // 读取完整资金来源时须并入同角色组合的整账资格。
    additional: Option<Permission>,
}

impl ManagementSource {
    // 登记只按固定范围判断、不另需整账资格的审批来源。
    const fn scoped(permission: Permission, subjects: &'static [&'static str]) -> Self {
        Self { permission, subjects, additional: None }
    }

    // 登记管理完整资金记录时还须同角色具有整账权限的审批来源。
    const fn whole_ledger(permission: Permission, subjects: &'static [&'static str]) -> Self {
        Self { permission, subjects, additional: Some(policy_permission::FINANCE_LEDGER_READ) }
    }

    /// 构造管理资格组合时追加该真实来源要求的整账资格。
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

// 按冻结主体类型枚举允许的来源，供管理分区和升级候选共用。
fn subject_sources(subject: &str) -> impl Iterator<Item = &'static ManagementSource> {
    MANAGEMENT_SOURCES.iter().filter(move |source| source.subjects.contains(&subject))
}

// 按冻结类型及真实资源定位来源政策，两个条件须同时匹配。
fn subject_source(subject: &str, resource: &str) -> Option<&'static ManagementSource> {
    subject_sources(subject).find(|source| source.permission.resource() == resource)
}

/// 按冻结主体和真实来源的登记顺序展开审批管理列表分区，附上该类型的运行管理权限。
pub(super) fn management_partitions()
-> impl Iterator<Item = (&'static str, &'static Permission, &'static ManagementSource)> {
    approval_admin::TYPES.iter().flat_map(|(subject, _, admin)| {
        let subject = *subject;
        subject_sources(subject).map(move |source| (subject, admin, source))
    })
}

// 从真实实例的冻结类型和唯一完整来源确定管理资格；升级绑定使用定义管理，其余使用运行管理。
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

/// 按已登记真实资源取得固定来源动作，供候选加载现存来源。
pub(super) fn source_permission(resource: &str) -> Result<Permission> {
    MANAGEMENT_SOURCES
        .iter()
        .find(|source| source.permission.resource() == resource)
        .map(|source| source.permission.clone())
        .ok_or_else(|| Error::ValidationError("审批管理的真实来源未接入授权".into()))
}

/// 升级绑定前确认候选只带一个真实来源引用；未给资源时仅允许从该类型的唯一合法来源推断。
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

/// 把管理入口转换为原业务动作，与实例列表及该冻结类型的管理权限组成同角色资格。
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

// 审批管理取证时要求同一启用角色授予整组操作权限，否则拒绝。
fn ensure_admin(required: &[Permission], qualified: &QualifiedPermissions) -> Result<()> {
    if qualified.same_role(required) {
        Ok(())
    } else {
        Err(Error::Forbidden("当前账号不具备该主体类型的同角色完整审批管理资格".into()))
    }
}

impl AuthorizationService {
    /// 根审批对象取证后沿原执行器检查真实来源。普通本人读取命中时不另查当前来源；库存撤回走专用条件。
    /// 调用方已确认是受支持的审批入口，或固定使用审批管理读取入口。
    pub(super) async fn approval_sources(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccessProof>> {
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

    // 在原读取阶段核对冻结类型管理资格、完整来源范围和可用记录补权，保留最早到期时点供交付检查。
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

    // 普通审批读取未命中时，沿原快照重新加载管理分支需要的真实来源，要求实例版本和冻结类型保持不变。
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

// 按发起人、真实已决定参与人或当前指派判断普通审批读取；当前指派还须同角色具备读取及决定权限。
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
    /// 由具备完整审批资格的角色解析真实来源并签发原阶段证明，不另要求来源菜单权限；记录版本、期限和复验参数。
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

    /// 以同一审批资格组合解析来源范围并收集版本和期限，供对象和列表共用；采购工作项详情沿用其专门条件。
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
            scopes.push((scope_source, resolution.include(scope)));
        }

        if context.permission == work_item::DETAIL && source == &purchase_order::DETAIL {
            let scope =
                &scopes.first().ok_or_else(|| Error::ValidationError("任务采购来源范围未加载".into()))?.1;
            return rules::data_condition(source, Some(scope));
        }
        scoped_source_condition(source, &scopes)
    }
}

/// 枚举管理资格须解释的实际来源范围，按权限去重并保留首次出现顺序。
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

/// 构造审批对象和管理列表共用的来源条件，保留完整性要求及政策允许的记录读取补充。
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

    // 构造带冻结类型及唯一业务来源的审批实例样本，供管理资格测试。
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

    // 验证管理权限必须匹配冻结单据类型，且与完整入口资格来自同一启用角色。
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

    // 验证升级候选只能省略唯一合法的来源资源，不能隐式选择多来源分区。
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

    // 验证升级绑定要求定义管理资格，并拒绝来源缺失或与冻结类型错配。
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
