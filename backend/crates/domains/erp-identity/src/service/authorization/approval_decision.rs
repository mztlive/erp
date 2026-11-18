//! 审批决定沿精确任务的冻结实例与真实订单来源执行固定范围检查。
use std::collections::BTreeSet;

use erp_permissions::{approval_instance, purchase_order, sales_order};
use persistence_core::Executor;

use super::AuthorizationService;
use super::targets::TargetFacts;
use super::work_items::exact_source;
use crate::entity::authorization::ObjectFacts;
use crate::entity::resource_access::{GrantTarget, ResourceKind, SourceKey};
use crate::repository::authorization::AccessProof;
use crate::{Error, Permission, Result};

impl AuthorizationService {
    // 办理角色同时承担来源范围；其他实例的读取授权不能放行本次决定。
    // 本次决定所使用的完整来源证明；其他权限不追加证明。
    //
    // 来源或冻结实例缺失、范围未命中、资格或阶段变化时拒绝。
    //
    // proof为本次办理资格，facts为精确任务及其冻结实例、真实来源，executor为原快照。
    pub(super) async fn approval_decision_sources(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccessProof>> {
        if proof.context.permission != approval_instance::SUBMIT_DECISION {
            return Ok(vec![]);
        }

        let mut proofs = Vec::new();
        for task in facts.roots() {
            let Some((permission, resource, rule)) = order_source(task)? else { continue };
            let source = exact_source(task, "subject_source", resource.code())?;
            let instance = exact_source(task, "decision_instance", "approval_instance")?;
            if instance.subject_type != task.subject_type {
                return Err(Error::ValidationError("审批决定的冻结实例与任务类型不符".into()));
            }

            let access = self
                .resolve_approval_source(
                    &proof.context,
                    &permission,
                    &[approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION],
                    &proof.qualified,
                    executor,
                )
                .await?;
            self.require_decision_source(&access, source, instance, resource, rule, executor).await?;
            proofs.push(access);
        }
        Ok(proofs)
    }
    // 审批决定先检查常规来源范围；未命中时，只允许该真实实例为当前账号建立的有效记录读取授权补足。
    async fn require_decision_source(
        &self,
        proof: &AccessProof,
        source: &ObjectFacts,
        instance: &ObjectFacts,
        resource: ResourceKind,
        rule: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut grants = BTreeSet::new();
        if proof.rule.allows_object(proof.context.actor.id(), source, &grants)? {
            return Ok(());
        }

        let key = SourceKey {
            rule: rule.into(),
            business_type: "approval_instance".into(),
            business_id: instance.id.clone(),
            recipient: proof.context.actor.id().into(),
        };
        let target = GrantTarget { resource, id: source.id.clone() };
        if self.facilities.source_grant(&key, &target, executor).await? {
            grants.insert((resource, source.id.clone()));
        }

        if !proof.rule.allows_object(proof.context.actor.id(), source, &grants)? {
            return Err(Error::Forbidden("没有审批单据的读取资格".into()));
        }
        Ok(())
    }
}

// 仅将销售单与采购单映射到审批分配的来源范围；其他单据按既有办理规则判断，不额外打开来源分支。
fn order_source(task: &ObjectFacts) -> Result<Option<(Permission, ResourceKind, &'static str)>> {
    Ok(match task.subject_type.get()?.as_str() {
        "sales_order" => {
            Some((sales_order::DETAIL, ResourceKind::SalesOrder, "sales_order_approval_assigned"))
        },
        "purchase_order" => {
            Some((purchase_order::DETAIL, ResourceKind::PurchaseOrder, "purchase_order_approval_assigned"))
        },
        "voucher_sales_order"
        | "sales_change_order"
        | "purchase_change_order"
        | "stock_adjustment"
        | "sales_invoice_request"
        | "purchase_payment_request"
        | "customer_receipt"
        | "customer_refund"
        | "supplier_refund"
        | "receipt_reversal"
        | "payment_reversal" => None,
        _ => return Err(Error::ValidationError("审批决定的冻结类型未登记".into())),
    })
}
