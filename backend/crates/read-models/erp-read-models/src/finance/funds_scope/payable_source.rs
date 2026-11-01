//! 应付关联保持真实来源类型，结算来源不得退化为空责任事实。

use std::collections::{BTreeSet, HashMap};

use erp_finance::entity::payable::PayableSourceType;
use erp_identity::access_control::ScopedObject;
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::repository::prelude::*;
use persistence_core::Executor;

use super::authorization::{FundsAccess, FundsAuthorization};
use super::rows::LinkedPurchaseFact;
use crate::Result;

const SETTLEMENT_PREFIX: &str = "supplier_settlement_statement:";

/// 内部关联键包含真实来源类型，避免相同主键在采购与结算域间串权。
///
/// # 参数
/// * `source` - 应付来源类型。
/// * `id` - 来源主键。
///
/// # 返回
/// 采购单返回原主键；供应商结算返回带 `supplier_settlement_statement:` 前缀的关联键。
///
/// # 错误
/// 不返回错误。
pub(super) fn source_key(source: PayableSourceType, id: &str) -> String {
    match source {
        PayableSourceType::PurchaseOrder => id.to_owned(),
        PayableSourceType::SupplierSettlement => format!("{SETTLEMENT_PREFIX}{id}"),
    }
}

impl FundsAccess {
    /// 有界批量装载结算当前责任，加入与采购共用的关联事实集合。
    ///
    /// # 参数
    /// * `ids` - 带结算前缀的来源关联键；其他键被忽略。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 返回存在的结算来源事实；缺失来源不补空事实。
    ///
    /// # 错误
    /// 结算单读取失败时返回对应错误。
    pub(super) async fn settlement_fact_map(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, LinkedPurchaseFact>> {
        let ids = ids
            .iter()
            .filter_map(|id| id.strip_prefix(SETTLEMENT_PREFIX).map(str::to_owned))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut facts = HashMap::new();
        for chunk in ids.chunks(500) {
            for statement in
                self.db.supplier_settlement_statements().find_statements_by_ids(chunk, executor).await?
            {
                facts.insert(
                    source_key(PayableSourceType::SupplierSettlement, &statement.base.id),
                    LinkedPurchaseFact {
                        owner_user_id: Some(statement.prepared_by),
                        business_org_unit_id: statement.business_org_unit_id,
                        version: statement.base.version,
                        document_no: statement.statement_no,
                    },
                );
            }
        }
        Ok(facts)
    }
}

impl FundsAuthorization {
    /// 每条应付关联仅采用其实际来源范围，未知或缺失来源不获得资格。
    ///
    /// # 参数
    /// * `facts` - 已装载的采购或结算来源事实。
    ///
    /// # 返回
    /// 始终返回 `Some`，内容是被对应来源范围覆盖的关联键；没有命中时为空集合。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn payable_ids(
        &self,
        facts: &HashMap<String, LinkedPurchaseFact>,
    ) -> Option<BTreeSet<String>> {
        Some(
            facts
                .iter()
                .filter(|(id, fact)| self.payable_fact_allowed(id, fact))
                .map(|(id, _)| id.clone())
                .collect(),
        )
    }

    /// 使用真实来源责任校验单一应付关联，不采用另一来源的组织授权。
    ///
    /// # 参数
    /// * `id` - 带类型的来源关联键。
    /// * `fact` - 该来源的当前责任事实。
    ///
    /// # 返回
    /// 结算键使用结算范围，其他键使用采购上下文；范围未装配或不覆盖该责任时返回 false。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn payable_fact_allowed(&self, id: &str, fact: &LinkedPurchaseFact) -> bool {
        let scope = if id.starts_with(SETTLEMENT_PREFIX) {
            let Some(scope) = self.settlement.as_ref() else {
                return false;
            };
            scope
        } else {
            if self.purchase_scope.is_none() {
                return false;
            }
            &self.context
        };
        scope.scope.allows(
            &ScopedObject {
                owned: fact.owner_user_id.as_deref() == Some(scope.user_id.as_str()),
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: Some(&fact.business_org_unit_id),
                settlement_party_id: None,
                warehouse_id: None,
            },
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_identity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::service::access_control::resolve::AuthorizedDataScope;
    use erp_procurement::repository::purchase_order::scope::PurchaseReadScope;
    use erp_sales::repository::sales_order::scope::SalesReadScope;

    use super::*;

    fn context(resource: &str, organization: &str) -> AuthorizedDataScope {
        AuthorizedDataScope {
            user_id: "user".into(),
            resource: resource.into(),
            action: "list".into(),
            scope: ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: [organization.into()].into(),
                    ..Default::default()
                }],
                user_limit: None,
            },
            role_scopes: Default::default(),
            organizations: Default::default(),
            policy_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(1),
        }
    }

    fn authorization() -> FundsAuthorization {
        FundsAuthorization {
            sales: SalesReadScope::default(),
            ledger_read: false,
            purchase_scope: Some(PurchaseReadScope::default()),
            context: context("payable_account", "purchase-org"),
            settlement: Some(context("payable_account", "settlement-org")),
            no_scope: false,
        }
    }

    fn fact(organization: &str) -> LinkedPurchaseFact {
        LinkedPurchaseFact {
            owner_user_id: Some("other".into()),
            business_org_unit_id: organization.into(),
            version: 3,
            document_no: "source-no".into(),
        }
    }

    #[test]
    fn purchase_and_settlement_with_same_id_use_their_own_boundary() {
        let access = authorization();
        let purchase = source_key(PayableSourceType::PurchaseOrder, "same-id");
        let settlement = source_key(PayableSourceType::SupplierSettlement, "same-id");
        assert_ne!(purchase, settlement);
        assert!(access.payable_fact_allowed(&purchase, &fact("purchase-org")));
        assert!(!access.payable_fact_allowed(&purchase, &fact("settlement-org")));
        assert!(access.payable_fact_allowed(&settlement, &fact("settlement-org")));
        assert!(!access.payable_fact_allowed(&settlement, &fact("purchase-org")));
    }

    #[test]
    fn missing_source_is_not_added_to_authorized_shares() {
        let access = authorization();
        let settlement = source_key(PayableSourceType::SupplierSettlement, "settlement");
        let facts =
            [(settlement.clone(), fact("settlement-org")), ("foreign".into(), fact("outside"))].into();
        assert_eq!(access.payable_ids(&facts), Some([settlement].into()));
        assert_eq!(access.payable_ids(&HashMap::new()), Some(BTreeSet::new()));
    }

    #[test]
    fn ledger_duty_keeps_unallocated_candidates_without_any_source_scope() {
        let mut access = authorization();
        access.context.scope.role_clauses.clear();
        access.settlement = None;
        assert!(access.empty());
        access.ledger_read = true;
        assert!(!access.empty());
        assert_eq!(access.payable_ids(&HashMap::new()), Some(BTreeSet::new()));
    }
}
