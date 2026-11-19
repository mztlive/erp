//! 管理列表按冻结类型和唯一真实来源编译完整对象条件。
use erp_permissions::internal;
use persistence_core::Executor;

use super::approvals::{
    management_partitions, management_permissions, management_scope_requests, scoped_source_condition,
};
use super::resolve::Resolution;
use super::{AuthorizationService, rules};
use crate::entity::authorization::{AccessContext, AccessRule, RelationKey, RelationMatch};
use crate::{Permission, QualifiedPermissions, Result};

// 仅用冻结主体类型和来源资源生成关联键，要求管理列表分区匹配唯一完整来源。
fn relation(subject: &str, source: &Permission, rule: AccessRule) -> Result<AccessRule> {
    Ok(AccessRule::Related {
        relation: RelationKey::parse(format!("management_{subject}_{}", source.resource()))?,
        resource: source.resource().into(),
        mode: RelationMatch::ExactlyOne,
        rule: Box::new(rule),
    })
}

/// 启动时展开全部审批管理分区及嵌套来源，供校验 Mapping 能力，不按本次账号资格删减。
pub(super) fn query_binding_condition() -> Result<AccessRule> {
    let mut partitions = Vec::new();
    for (subject, _, policy) in management_partitions() {
        let source = &policy.permission;
        let scopes = management_scope_requests(source)?
            .into_iter()
            .map(|source| {
                let scope = rules::capability_scope(&source)?;
                Ok((source, scope))
            })
            .collect::<Result<Vec<_>>>()?;
        partitions.push(relation(subject, source, scoped_source_condition(source, &scopes)?)?);
    }
    Ok(AccessRule::any(partitions))
}

impl AuthorizationService {
    /// 编译审批管理列表时，用同次资格仅展开当前账号有权管理的类型，并由合格角色解释本分区的真实来源范围。
    pub(super) async fn approval_query_condition(
        &self,
        context: &AccessContext,
        qualified: &QualifiedPermissions,
        resolution: &mut Resolution,
        executor: &mut dyn Executor,
    ) -> Result<AccessRule> {
        let mut partitions = Vec::new();
        for (subject, admin, policy) in management_partitions() {
            let source = &policy.permission;
            let mut required = management_permissions(&internal::APPROVAL_MANAGEMENT_READ, admin);
            policy.add_qualification(&mut required);
            if !qualified.same_role(&required) {
                continue;
            }

            let source_context = AccessContext { resource: source.resource().into(), ..context.clone() };
            let rule = self
                .approval_source_condition(
                    &source_context,
                    source,
                    &required,
                    qualified,
                    resolution,
                    executor,
                )
                .await?;
            partitions.push(relation(subject, source, rule)?);
        }
        Ok(AccessRule::any(partitions))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证复合审批来源请求按权限去重，并保留各权限首次出现的顺序。
    #[test]
    fn scope_requests_keep_the_first_occurrence_of_each_scope_permission() {
        use erp_permissions::{purchase_order, returns, sales_order, supplier_settlement};

        assert_eq!(
            management_scope_requests(&returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT).unwrap(),
            [sales_order::DETAIL, purchase_order::DETAIL, supplier_settlement::STATEMENT_LIST]
        );
    }

    // 验证启动绑定完整保留有序的类型和来源分区，且每个分区都要求唯一完整来源。
    #[test]
    fn binding_preserves_every_approval_partition_in_order() {
        let AccessRule::Any(partitions) = query_binding_condition().unwrap() else {
            panic!("管理列表必须保留全部固定审批分区");
        };
        let keys = partitions
            .iter()
            .map(|partition| match partition {
                AccessRule::Related { relation, mode: RelationMatch::ExactlyOne, .. } => relation.as_str(),
                _ => panic!("管理列表分区必须要求唯一完整来源"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "management_sales_order_sales_order",
                "management_voucher_sales_order_sales_order",
                "management_sales_change_order_sales_order",
                "management_purchase_order_purchase_order",
                "management_purchase_change_order_purchase_order",
                "management_stock_adjustment_stock_adjustment",
                "management_sales_invoice_request_sales_invoice_request",
                "management_purchase_payment_request_purchase_payment_request",
                "management_customer_receipt_customer_receipt",
                "management_customer_refund_customer_receipt",
                "management_customer_refund_receivable_account",
                "management_supplier_refund_supplier_payment",
                "management_supplier_refund_payable_account",
                "management_receipt_reversal_customer_receipt",
                "management_payment_reversal_supplier_payment",
            ]
        );
    }
}
