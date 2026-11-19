//! 工作项详情将真实订单、门户材料和审批管理来源纳入同次数据库过滤。
use erp_permissions::{approval_instance, internal, purchase_order, sales_order, work_item};
use persistence_core::Executor;

use super::resolve::Resolution;
use super::targets::{SourceEvidence, TargetFacts};
use super::work_items::{
    PORTAL_REQUESTS, PortalRequest, decision_qualified, exact_source, order_resource, portal_request,
};
use super::{AuthorizationService, approval_query, materials, rules};
use crate::entity::authorization::{
    AccessContext, AccessFact, AccessRule, AccessTarget, ObjectFacts, RelationKey, RelationMatch,
};
use crate::repository::authorization::AccessProof;
use crate::{Error, Permission, QualifiedPermissions, Result};

enum DetailPartition {
    Other,
    Order(&'static str, Permission),
    Portal(&'static PortalRequest),
    ApprovalOwner,
    ApprovalManagement,
}

fn detail_partitions() -> impl Iterator<Item = DetailPartition> {
    [
        DetailPartition::Other,
        DetailPartition::Order("detail_sales", sales_order::DETAIL),
        DetailPartition::Order("detail_purchase", purchase_order::DETAIL),
    ]
    .into_iter()
    .chain(PORTAL_REQUESTS.iter().map(DetailPartition::Portal))
    .chain([DetailPartition::ApprovalOwner, DetailPartition::ApprovalManagement])
}

impl DetailPartition {
    // 每类详情分支应以唯一完整来源为准；自引用只用于无需额外来源的任务分区。
    fn related(&self, rule: AccessRule) -> Result<AccessRule> {
        let (key, resource) = match self {
            Self::Other => ("detail_other", "work_item"),
            Self::Order(key, source) => (*key, source.resource()),
            Self::Portal(request) => (request.detail_key, "supplier_portal_material"),
            Self::ApprovalOwner => ("detail_approval_owner", "work_item"),
            Self::ApprovalManagement => ("detail_approval_management", "approval_instance"),
        };
        Ok(AccessRule::Related {
            relation: RelationKey::parse(key)?,
            resource: resource.into(),
            mode: RelationMatch::ExactlyOne,
            rule: Box::new(rule),
        })
    }
}

fn approval_owner(can_decide: bool) -> AccessRule {
    let owned = AccessRule::Fact(AccessFact::Owned);
    if can_decide {
        owned
    } else {
        AccessRule::all(vec![owned, AccessRule::Fact(AccessFact::Completed(true))])
    }
}

// 启动时检查全部详情来源及独立管理过滤，不按账号资格省略分支。
pub(super) fn binding_condition(base: AccessRule) -> Result<AccessRule> {
    let partitions = detail_partitions()
        .map(|partition| {
            let rule = match &partition {
                DetailPartition::Other => AccessRule::Allow,
                DetailPartition::Order(_, source) => {
                    rules::data_condition(source, Some(&rules::capability_scope(source)?))?
                },
                DetailPartition::Portal(request) => materials::binding_condition(&request.read)?,
                DetailPartition::ApprovalOwner => approval_owner(false),
                DetailPartition::ApprovalManagement => approval_query::query_binding_condition()?,
            };
            partition.related(rule)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut checks = vec![base, AccessRule::any(partitions)];
    checks.extend(rules::management_binding_condition(&work_item::DETAIL)?);
    Ok(AccessRule::all(checks))
}

impl AuthorizationService {
    // 为详情查询将来源数据范围绑定到根任务的同次身份、组织有效期、策略修订和执行阶段。
    pub(super) async fn work_item_query_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        base: AccessRule,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let can_read_approvals = qualified.permissions().covers_one(&approval_instance::COPIES_HISTORY);
        let mut partitions = Vec::new();
        for partition in detail_partitions() {
            let rule = match &partition {
                DetailPartition::Other => AccessRule::Allow,
                DetailPartition::Order(_, source) => {
                    self.approval_source_condition(
                        context,
                        source,
                        &[work_item::DETAIL],
                        qualified,
                        resolution,
                        executor,
                    )
                    .await?
                },
                DetailPartition::Portal(request) => {
                    if !rules::qualified(&request.read, qualified) {
                        continue;
                    }
                    let source = AccessContext {
                        permission: request.read.clone(),
                        resource: "supplier_portal_material".into(),
                        ..context.clone()
                    };
                    let rule = self.material_condition(&source, qualified, resolution, executor).await?;
                    AccessRule::Independent { permission: request.read.clone(), rule: Box::new(rule) }
                },
                DetailPartition::ApprovalOwner | DetailPartition::ApprovalManagement
                    if !can_read_approvals =>
                {
                    continue;
                },
                DetailPartition::ApprovalOwner => approval_owner(decision_qualified(qualified)),
                DetailPartition::ApprovalManagement => {
                    let management = AccessContext {
                        permission: internal::APPROVAL_MANAGEMENT_READ,
                        resource: "approval_instance".into(),
                        ..context.clone()
                    };
                    self.approval_query_condition(&management, qualified, resolution, executor).await?
                },
            };
            partitions.push(partition.related(rule)?);
        }
        Ok(AccessRule::all(vec![base, AccessRule::any(partitions)]))
    }

    // 精确任务详情必须执行其固定来源权限；普通列表只执行自己的条件，不被详情来源收窄。
    pub(super) async fn work_item_detail_sources(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<SourceEvidence> {
        let mut sources = SourceEvidence::default();
        for object in facts.roots() {
            if object.subject_type.get()? == "DOCUMENT_APPROVAL" {
                sources.extend(self.approval_task_sources(proof, qualified, object, executor).await?);
            } else if let Some(resource) = order_resource(object.subject_source_resource.get()?) {
                sources
                    .proofs
                    .push(self.work_item_order(proof, qualified, facts, object, resource, executor).await?);
            } else if object.subject_source_resource.get()? == "supplier_portal_request" {
                sources.proofs.push(self.portal_task_read(proof, qualified, facts, object, executor).await?);
            }
        }
        Ok(sources)
    }

    // 供应商任务详情以材料原有读取资格判定，完整任务办理另由其固定入口核验审核资格。
    async fn portal_task_read(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        facts: &TargetFacts,
        object: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let source = exact_source(object, "portal_source", "supplier_portal_material")?;
        let request = portal_request(source.subject_type.get()?)
            .ok_or_else(|| Error::ValidationError("任务材料真实申请类型未登记".into()))?;
        let permission = request.read.clone();
        if !rules::qualified(&permission, qualified) {
            return Err(Error::Forbidden("当前账号不具备任务材料读取资格".into()));
        }

        let context = AccessContext {
            permission,
            resource: source.resource.clone(),
            model: None,
            ..proof.context.clone()
        };
        let access = self.resolve_context(context, None, false, qualified, executor).await?;
        self.check_work_item_source(&access, facts, source, executor).await?;
        Ok(access)
    }

    // 原审批责任人按已有读取及决定资格核对来源；管理读者则须通过真实类型的管理资格和业务来源数据范围。
    pub(super) async fn approval_task_sources(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        task: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<SourceEvidence> {
        if !qualified.permissions().covers_one(&approval_instance::COPIES_HISTORY) {
            return Err(Error::Forbidden("当前账号不具备审批任务读取资格".into()));
        }
        if task.owner_id.get()?.as_deref() == Some(proof.context.actor.id())
            && (*task.completed.get()? || decision_qualified(qualified))
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
        let access = self.resolve_context(context, None, false, qualified, executor).await?;

        let target = AccessTarget::record(&instance.id)?;
        let facts =
            TargetFacts::load(&self.facts, &access.context, &access.resolved.rules.root, target, executor)
                .await?;

        let mut proofs = self.approval_sources(&access, qualified, &facts, executor).await?;
        let context = access.context.clone();
        proofs.push(access);
        Ok(SourceEvidence { proofs, facts: vec![(context, facts)] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::authorization::QueryOutput;

    #[test]
    fn binding_checks_management_after_all_detail_partitions() {
        let binding = rules::binding_condition(&work_item::DETAIL, &QueryOutput::Object).unwrap();
        let AccessRule::All(checks) = binding else {
            panic!("任务详情绑定必须检查根对象、来源分区和独立管理范围");
        };
        let [_, AccessRule::Any(partitions), management] = checks.as_slice() else {
            panic!("独立管理范围必须在完整详情规则之后校验");
        };
        assert_eq!(
            management,
            &rules::data_condition(
                &work_item::MANAGE,
                Some(&rules::capability_scope(&work_item::MANAGE).unwrap())
            )
            .unwrap()
        );
        let keys = partitions
            .iter()
            .map(|partition| match partition {
                AccessRule::Related { relation, mode: RelationMatch::ExactlyOne, .. } => relation.as_str(),
                _ => panic!("详情分区必须要求唯一完整来源"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "detail_other",
                "detail_sales",
                "detail_purchase",
                "detail_portal_application",
                "detail_portal_offering",
                "detail_portal_new_product",
                "detail_portal_cooperation",
                "detail_approval_owner",
                "detail_approval_management",
            ]
        );
    }
}
