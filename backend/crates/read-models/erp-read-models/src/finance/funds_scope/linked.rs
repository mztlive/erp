//! 资金查询的组织条件与关联责任投影。

use std::collections::{BTreeSet, HashMap};

use application_core::QueryIds;
use erp_finance::ports::funds_scope::FundsDataScopePort;
use persistence_core::Executor;

use super::rows::{LinkedOrderId, OrderTuple};
use crate::{Error, Result};

/// 展开已提供的组织条件；未提供时不读取组织事实。
///
/// # 参数
/// * `scope` - 已装配的资金授权端口。
/// * `ids` - 请求中的组织条件；`None` 表示不附加组织筛选。
/// * `include_descendants` - 是否包含有效下级；缺省为否。
/// * `executor` - 本次查询沿用的执行器。
///
/// # 返回
/// 未提供条件时返回 `None`；已提供时返回排序去重后的组织，即使展开结果为空也保留 `Some`。
///
/// # 错误
/// 组织展开失败时保留领域错误分类。
pub(super) async fn organization_condition(
    scope: &dyn FundsDataScopePort,
    ids: Option<&QueryIds>,
    include_descendants: Option<bool>,
    executor: &mut dyn Executor,
) -> Result<Option<Vec<String>>> {
    let Some(ids) = ids else {
        return Ok(None);
    };
    let expanded = scope
        .expand_org_units(ids.as_slice(), include_descendants.unwrap_or(false), executor)
        .await
        .map_err(Error::from)?;
    Ok(Some(expanded.into_iter().collect()))
}

/// 把关联来源按首次出现顺序投影为责任元组。
///
/// # 参数
/// * `document_id` / `version` - 当前资金单据身份与版本。
/// * `orders` - 按原分配顺序提供的来源身份。
/// * `facts` - 当前查询取得的来源责任事实。
/// * `responsibility` - 显式映射来源负责人及组织，保留采购负责人缺失语义。
///
/// # 返回
/// 按来源键去重后的元组；缺失来源事实保留未分配元组。`None` 和空字符串沿用同一去重键。
///
/// # 错误
/// 不返回错误。
pub(super) fn linked_order_tuples<'a, F>(
    document_id: &str,
    version: u64,
    orders: impl IntoIterator<Item = &'a LinkedOrderId>,
    facts: &HashMap<String, F>,
    responsibility: impl Fn(&F) -> (Option<String>, String),
) -> Vec<OrderTuple> {
    let mut seen = BTreeSet::new();
    let mut tuples = Vec::new();
    for order in orders {
        if !seen.insert(order.clone().unwrap_or_default()) {
            continue;
        }
        let tuple = match order.as_ref().and_then(|id| facts.get(id)) {
            Some(fact) => {
                let (owner, org) = responsibility(fact);
                (order.clone(), owner, Some(org), document_id.to_string(), version)
            },
            None => (None, None, None, document_id.to_string(), version),
        };
        tuples.push(tuple);
    }
    tuples
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use application_core::AuditActor;
    use async_trait::async_trait;
    use erp_core::common::time::Instant;
    use erp_finance::ports::funds_scope::FundsResolvedScope;
    use erp_finance::{Error as FinanceError, Result as FinanceResult};
    use persistence_core::NoTransaction;

    use super::super::rows::{LinkedPurchaseFact, LinkedSalesFact};
    use super::*;

    #[derive(Default)]
    struct OrganizationPort {
        calls: Mutex<Vec<(Vec<String>, bool)>>,
        expanded: BTreeSet<String>,
        fail: bool,
    }

    #[async_trait]
    impl FundsDataScopePort for OrganizationPort {
        async fn resolve(
            &self,
            _actor: &AuditActor,
            _resource: &str,
            _action: &str,
            _executor: &mut dyn Executor,
        ) -> FinanceResult<FundsResolvedScope> {
            Err(FinanceError::Internal("未使用的授权解析".into()))
        }

        async fn expand_org_units(
            &self,
            ids: &[String],
            include_descendants: bool,
            _executor: &mut dyn Executor,
        ) -> FinanceResult<BTreeSet<String>> {
            self.calls.lock().unwrap().push((ids.to_vec(), include_descendants));
            if self.fail {
                return Err(FinanceError::ValidationError("组织不存在".into()));
            }
            Ok(self.expanded.clone())
        }

        async fn org_member_ids(
            &self,
            _ids: &BTreeSet<String>,
            _at: Instant,
            _executor: &mut dyn Executor,
        ) -> FinanceResult<Vec<String>> {
            Err(FinanceError::Internal("未使用的成员读取".into()))
        }
    }

    fn ids() -> QueryIds {
        serde_json::from_value(serde_json::json!("org-2,org-1")).unwrap()
    }

    #[tokio::test]
    async fn missing_organization_condition_skips_port() {
        let port = OrganizationPort { fail: true, ..Default::default() };
        let condition = organization_condition(&port, None, Some(true), &mut NoTransaction).await.unwrap();
        assert_eq!(condition, None);
        assert!(port.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn organization_condition_defaults_to_direct_units_and_retains_sorted_result() {
        let port = OrganizationPort {
            expanded: BTreeSet::from(["org-2".into(), "org-1".into()]),
            ..Default::default()
        };
        let condition = organization_condition(&port, Some(&ids()), None, &mut NoTransaction).await.unwrap();
        assert_eq!(condition, Some(vec!["org-1".into(), "org-2".into()]));
        assert_eq!(*port.calls.lock().unwrap(), vec![(vec!["org-1".into(), "org-2".into()], false)]);
    }

    #[tokio::test]
    async fn empty_expansion_stays_an_explicit_condition_and_passes_descendant_option() {
        let port = OrganizationPort::default();
        let condition =
            organization_condition(&port, Some(&ids()), Some(true), &mut NoTransaction).await.unwrap();
        assert_eq!(condition, Some(Vec::new()));
        assert_eq!(*port.calls.lock().unwrap(), vec![(vec!["org-1".into(), "org-2".into()], true)]);
    }

    #[tokio::test]
    async fn organization_error_keeps_validation_classification() {
        let port = OrganizationPort { fail: true, ..Default::default() };
        let error = organization_condition(&port, Some(&ids()), None, &mut NoTransaction).await.unwrap_err();
        assert!(matches!(error, Error::ValidationError(message) if message == "组织不存在"));
    }

    #[test]
    fn sales_tuples_keep_first_source_order_and_missing_fact_rows() {
        let facts = HashMap::from([
            ("sales-1".into(), sales_fact("owner-1")),
            ("sales-2".into(), sales_fact("owner-2")),
        ]);
        let orders = [
            Some("sales-2".into()),
            Some("missing-1".into()),
            Some("sales-1".into()),
            Some("sales-2".into()),
            Some("missing-2".into()),
            None,
            None,
        ];
        let tuples = linked_order_tuples("receipt-1", 7, &orders, &facts, |fact| {
            (Some(fact.owner_user_id.clone()), fact.business_org_unit_id.clone())
        });
        assert_eq!(
            tuples,
            vec![
                tuple(Some("sales-2"), Some("owner-2"), Some("org-1")),
                tuple(None, None, None),
                tuple(Some("sales-1"), Some("owner-1"), Some("org-1")),
                tuple(None, None, None),
                tuple(None, None, None),
            ]
        );
    }

    #[test]
    fn purchase_fact_without_owner_keeps_source_and_organization() {
        let facts = HashMap::from([(
            "purchase-1".into(),
            LinkedPurchaseFact {
                owner_user_id: None,
                business_org_unit_id: "org-1".into(),
                version: 3,
                document_no: "PO-1".into(),
            },
        )]);
        let orders = [Some("purchase-1".into())];
        let tuples = linked_order_tuples("receipt-1", 7, &orders, &facts, |fact| {
            (fact.owner_user_id.clone(), fact.business_org_unit_id.clone())
        });
        assert_eq!(tuples, vec![tuple(Some("purchase-1"), None, Some("org-1"))]);
    }

    #[test]
    fn empty_source_and_no_source_share_the_original_first_seen_key() {
        let facts = HashMap::from([("".into(), sales_fact("owner-1"))]);
        for (orders, expected) in [
            ([None, Some(String::new())], tuple(None, None, None)),
            ([Some(String::new()), None], tuple(Some(""), Some("owner-1"), Some("org-1"))),
        ] {
            let tuples = linked_order_tuples("receipt-1", 7, &orders, &facts, |fact| {
                (Some(fact.owner_user_id.clone()), fact.business_org_unit_id.clone())
            });
            assert_eq!(tuples, vec![expected]);
        }
    }

    #[test]
    fn empty_links_produce_no_responsibility_rows() {
        let orders: [LinkedOrderId; 0] = [];
        let tuples = linked_order_tuples(
            "receipt-1",
            7,
            &orders,
            &HashMap::<String, LinkedSalesFact>::new(),
            |fact| (Some(fact.owner_user_id.clone()), fact.business_org_unit_id.clone()),
        );
        assert!(tuples.is_empty());
    }

    fn sales_fact(owner: &str) -> LinkedSalesFact {
        LinkedSalesFact { owner_user_id: owner.into(), business_org_unit_id: "org-1".into(), version: 3 }
    }

    fn tuple(order: Option<&str>, owner: Option<&str>, org: Option<&str>) -> OrderTuple {
        (order.map(str::to_owned), owner.map(str::to_owned), org.map(str::to_owned), "receipt-1".into(), 7)
    }
}
