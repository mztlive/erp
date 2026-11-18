//! 工作项详情将真实订单、门户材料和审批管理来源纳入同次数据库过滤。
use erp_permissions::{approval_instance, internal, purchase_order, sales_order, work_item};
use persistence_core::Executor;

use super::resolve::Resolution;
use super::targets::{SourceEvidence, Target, TargetFacts};
use super::work_items::{decision_qualified, exact_source, order_resource};
use super::{AuthorizationService, approval_query, materials, rules};
use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, ObjectFacts, RelationKey, RelationMatch, ResourceTargets,
};
use crate::repository::authorization::AccessProof;
use crate::{Error, Permission, QualifiedPermissions, Result};

const PORTAL: &[(&str, Permission)] = &[
    ("detail_portal_application", internal::PORTAL_MATERIAL_READ),
    ("detail_portal_offering", internal::PORTAL_MATERIAL_READ_OFFERING),
    ("detail_portal_new_product", internal::PORTAL_MATERIAL_READ),
    ("detail_portal_cooperation", internal::PORTAL_MATERIAL_READ),
];

/// 每个分区都必须使用唯一完整来源；自引用分区只表达无需额外来源的真实任务。
fn related(key: &str, resource: &str, rule: AccessRule) -> Result<AccessRule> {
    Ok(AccessRule::Related {
        relation: RelationKey::parse(key)?,
        resource: resource.into(),
        mode: RelationMatch::ExactlyOne,
        rule: Box::new(rule),
    })
}

/// 启动校验覆盖所有详情分区；运行资格不能掩盖缺失来源 Mapping。
pub(super) fn binding_condition(base: AccessRule) -> Result<AccessRule> {
    let mut partitions = vec![related("detail_other", "work_item", AccessRule::Allow)?];
    for (key, source) in [("detail_sales", sales_order::DETAIL), ("detail_purchase", purchase_order::DETAIL)]
    {
        partitions.push(related(
            key,
            source.resource(),
            rules::data_condition(&source, Some(&rules::capability_scope(&source)?))?,
        )?);
    }

    for (key, permission) in PORTAL {
        partitions.push(related(key, "supplier_portal_material", materials::binding_condition(permission)?)?);
    }

    partitions.push(related(
        "detail_approval_owner",
        "work_item",
        AccessRule::all(vec![
            AccessRule::Fact(AccessFact::Owned),
            AccessRule::Fact(AccessFact::Completed(true)),
        ]),
    )?);
    partitions.push(related(
        "detail_approval_management",
        "approval_instance",
        approval_query::query_binding_condition()?,
    )?);

    Ok(AccessRule::all(vec![base, AccessRule::any(partitions)]))
}

impl AuthorizationService {
    /// 详情来源范围与原根共用账号资格、组织期限、授权修订和读取阶段。
    pub(super) async fn work_item_query_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        base: AccessRule,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let mut partitions = vec![related("detail_other", "work_item", AccessRule::Allow)?];
        for (key, source) in
            [("detail_sales", sales_order::DETAIL), ("detail_purchase", purchase_order::DETAIL)]
        {
            let rule = self
                .approval_source_condition(
                    context,
                    &source,
                    &[work_item::DETAIL],
                    qualified,
                    resolution,
                    executor,
                )
                .await?;
            partitions.push(related(key, source.resource(), rule)?);
        }

        self.portal_partitions(context, qualified, resolution, &mut partitions, executor).await?;

        if qualified.permissions().covers_one(&approval_instance::COPIES_HISTORY) {
            let owner = if decision_qualified(qualified) {
                AccessRule::Fact(AccessFact::Owned)
            } else {
                AccessRule::all(vec![
                    AccessRule::Fact(AccessFact::Owned),
                    AccessRule::Fact(AccessFact::Completed(true)),
                ])
            };
            partitions.push(related("detail_approval_owner", "work_item", owner)?);

            let management = AccessContext {
                permission: internal::APPROVAL_MANAGEMENT_READ,
                resource: "approval_instance".into(),
                ..context.clone()
            };
            let rule = self.approval_query_condition(&management, qualified, resolution, executor).await?;
            partitions.push(related("detail_approval_management", "approval_instance", rule)?);
        }

        Ok(AccessRule::all(vec![base, AccessRule::any(partitions)]))
    }

    /// 材料读取按真实申请类型使用既有读取资格，不追加审核资格。
    async fn portal_partitions(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        partitions: &mut Vec<AccessRule>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for (key, permission) in PORTAL {
            if !rules::qualified(permission, qualified) {
                continue;
            }
            let source = AccessContext {
                permission: permission.clone(),
                resource: "supplier_portal_material".into(),
                ..context.clone()
            };
            let rule = self.material_condition(&source, qualified, resolution, executor).await?;
            partitions.push(related(
                key,
                "supplier_portal_material",
                AccessRule::Independent { permission: permission.clone(), rule: Box::new(rule) },
            )?);
        }
        Ok(())
    }

    /// 对象入口沿同一来源范围和资格执行；普通列表不追加详情来源条件。
    pub(super) async fn work_item_detail_sources(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<SourceEvidence> {
        let mut sources = SourceEvidence::default();
        for object in facts.roots() {
            if object.subject_type.get()? == "DOCUMENT_APPROVAL" {
                sources.extend(self.approval_task_sources(proof, object, executor).await?);
            } else if let Some(resource) = order_resource(object.subject_source_resource.get()?) {
                sources.proofs.push(self.work_item_order(proof, facts, object, resource, executor).await?);
            } else if object.subject_source_resource.get()? == "supplier_portal_request" {
                sources.proofs.push(self.portal_task_read(proof, facts, object, executor).await?);
            }
        }
        Ok(sources)
    }

    /// 详情读取只要求材料原有读取资格；完整办理额外审核资格由办理入口执行。
    async fn portal_task_read(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        object: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let source = exact_source(object, "portal_source", "supplier_portal_material")?;
        let permission = match source.subject_type.get()?.as_str() {
            "offering_update" => internal::PORTAL_MATERIAL_READ_OFFERING,
            "offering_create" | "new_product" | "cooperation" => internal::PORTAL_MATERIAL_READ,
            _ => return Err(Error::ValidationError("任务材料真实申请类型未登记".into())),
        };
        if !rules::qualified(&permission, &proof.qualified) {
            return Err(Error::Forbidden("当前账号不具备任务材料读取资格".into()));
        }

        let context = AccessContext {
            permission,
            resource: source.resource.clone(),
            model: None,
            ..proof.context.clone()
        };
        let access = self.resolve_context(context, None, false, &proof.qualified, executor).await?;
        self.check_work_item_source(&access, facts, source, executor).await?;
        Ok(access)
    }

    /// 审批任务负责人按原读取和决定资格；其他读者必须满足真实类型管理及来源范围。
    pub(super) async fn approval_task_sources(
        &self,
        proof: &AccessProof,
        task: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<SourceEvidence> {
        if !proof.qualified.permissions().covers_one(&approval_instance::COPIES_HISTORY) {
            return Err(Error::Forbidden("当前账号不具备审批任务读取资格".into()));
        }
        if task.owner_id.get()?.as_deref() == Some(proof.context.actor.id())
            && (*task.completed.get()? || decision_qualified(&proof.qualified))
        {
            return Ok(SourceEvidence::default());
        }

        let instance = exact_source(task, "approval_instance", "approval_instance")?;
        let context = AccessContext {
            permission: internal::APPROVAL_MANAGEMENT_READ,
            resource: "approval_instance".into(),
            model: None,
            ..proof.context.clone()
        };
        let access = self.resolve_context(context, None, false, &proof.qualified, executor).await?;

        let targets = ResourceTargets::new(&[instance.id.as_str()])?;
        let facts = TargetFacts::load(
            &self.facts,
            &access.context,
            &access.rule,
            Target::Records(&targets),
            executor,
        )
        .await?;

        let mut proofs = self.approval_sources(&access, &facts, executor).await?;
        let context = access.context.clone();
        proofs.push(access);
        Ok(SourceEvidence { proofs, facts: vec![(context, facts)] })
    }
}
