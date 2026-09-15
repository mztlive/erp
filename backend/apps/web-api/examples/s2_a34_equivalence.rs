//! S2 A34 真实数据库等价性验收：公共单对象判定与真实 MongoDB 条件执行对拍。
//! 仅允许显式 ERP_TEST_MONGO_URI，使用随机库并在结束后删除；不用 hint，不用内存解释器做通过依据。
use std::collections::BTreeSet;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use erp_contract::ports::ContractScopeObject;
use erp_contract::service::contract::access::contract_scope;
use erp_contract::{ContractDataScopePort, ContractResolvedClause, ContractResolvedScope};
use erp_customer::ports::CustomerScopeObject;
use erp_customer::service::customer::access::customer_scope;
use erp_customer::{CustomerDataScopePort, CustomerResolvedClause, CustomerResolvedScope};
use erp_identity::access_control::{ResolvedScope, ScopeClause};
use erp_identity::service::access_control::resolve::AuthorizedDataScope;
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::adapters::{MongoContractDataScope, MongoCustomerDataScope, MongoPurchaseDataScope};
use erp_procurement::ports::PurchaseScopeObject;
use erp_procurement::service::purchase_order::access::purchase_scope;
use erp_procurement::{PurchaseDataScopePort, PurchaseResolvedClause, PurchaseResolvedScope};
use erp_read_models::sales_center::access::{SalesAccess, sales_scope};
use mongodb::bson::{Document, doc};
use mongodb::{Client, Collection, Database};

type Outcome<T = ()> = Result<T, Box<dyn Error>>;

const ROLE_MASKS: [u8; 9] = [0, 1, 2, 3, 4, 5, 8, 9, 15];
const SECOND_MASKS: [u8; 3] = [0, 2, 8];
const LIMITS: [i8; 6] = [-1, 0, 1, 2, 8, 15];
const ACTIONS: [&str; 3] = ["detail", "create", "update"];

#[tokio::main(flavor = "current_thread")]
async fn main() -> Outcome {
    let uri = std::env::var("ERP_TEST_MONGO_URI")?;
    let client = Client::with_uri_str(uri).await?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let db = client.database(&format!("erp_s2_a34_{suffix}"));
    let outcome = verify(&db).await;
    db.drop().await?;
    outcome
}

async fn verify(db: &Database) -> Outcome {
    let rbac = shared_rbac_service(db.clone());
    let customer_cases = verify_customer(db, &rbac).await?;
    println!("PASS a34_real_execution_customer {customer_cases} cases");
    let contract_cases = verify_contract(db, &rbac).await?;
    println!("PASS a34_real_execution_contract {contract_cases} cases");
    let purchase_cases = verify_purchase(db, &rbac).await?;
    println!("PASS a34_real_execution_purchase {purchase_cases} cases");
    let sales_cases = verify_sales(db).await?;
    println!("PASS a34_real_execution_sales_order {sales_cases} cases");
    println!(
        "LIMIT synthetic scratch documents in an isolated random database; production-volume and existing business-account acceptance remain separate"
    );
    Ok(())
}

async fn real_ids(collection: &Collection<Document>, filter: Document) -> Outcome<BTreeSet<String>> {
    let mut cursor = collection.find(filter).await?;
    let mut ids = BTreeSet::new();
    while cursor.advance().await? {
        let row: Document = cursor.deserialize_current()?;
        ids.insert(row.get_str("id")?.to_string());
    }
    Ok(ids)
}

fn selected(ids: &[String], bit: usize) -> Vec<String> {
    ids.iter().enumerate().filter(|(index, _)| index & bit != 0).map(|(_, id)| id.clone()).collect()
}

fn fixture_ids() -> Vec<String> {
    (0..16).map(|index| format!("o-{index}")).collect()
}

async fn verify_customer(db: &Database, rbac: &erp_identity::SharedRbacService) -> Outcome<usize> {
    let collection = db.collection::<Document>("s2_a34_customer");
    let ids = fixture_ids();
    let docs = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            doc! {
                "id": id,
                "customer_id": id,
                "owner_user_id": if index & 1 != 0 { "actor" } else { "other" },
                "business_org_unit_id": if index & 4 != 0 { "org-a" } else { "org-b" },
            }
        })
        .collect::<Vec<_>>();
    collection.insert_many(docs).await?;
    let port = MongoCustomerDataScope::new(db.clone(), rbac.clone());
    let clause = |mask: u8| CustomerResolvedClause {
        company: mask & 1 != 0,
        self_owned: mask & 2 != 0,
        collaborative: mask & 4 != 0,
        org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
    };
    let owned = selected(&ids, 1);
    let collaborating = selected(&ids, 2);
    let org_owned = vec![(vec!["org-a".into()], selected(&ids, 4))];
    let mut cases = 0;
    for action in ACTIONS {
        for role in ROLE_MASKS {
            for second in SECOND_MASKS {
                for limit in LIMITS {
                    let access = CustomerResolvedScope {
                        user_id: "actor".into(),
                        resource: "customer".into(),
                        action: action.into(),
                        role_clauses: vec![clause(role), clause(second)],
                        user_limit: (limit >= 0).then(|| clause(limit as u8)),
                        policy_version: 1,
                        organization_version: 1,
                        scope_version: "v1".into(),
                        as_of: erp_core::common::time::Instant::from_unix_secs(0),
                    };
                    let mut history = Vec::new();
                    if action == "detail" {
                        history = selected(&ids, 8);
                    }
                    let compiled =
                        customer_scope(&access, "actor", &owned, &collaborating, history, &org_owned);
                    let mut expected = BTreeSet::new();
                    for (index, id) in ids.iter().enumerate() {
                        let object = CustomerScopeObject {
                            owned: index & 1 != 0,
                            collaborating: index & 2 != 0,
                            historical_read_participant: index & 8 != 0,
                            org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                        };
                        if port.allows(&access, &object).expect("public decision") {
                            expected.insert(id.clone());
                        }
                    }
                    let actual = real_ids(&collection, compiled.document()).await?;
                    assert_eq!(
                        expected, actual,
                        "customer action={action} role={role} second={second} limit={limit}"
                    );
                    cases += 1;
                }
            }
        }
    }
    cases += missing_clause_cases(&collection, &ids, &port).await?;
    Ok(cases)
}

async fn missing_clause_cases(
    collection: &Collection<Document>,
    ids: &[String],
    port: &MongoCustomerDataScope,
) -> Outcome<usize> {
    let mut cases = 0;
    for action in ["detail", "create"] {
        for limit in [None, Some(0_u8), Some(1_u8)] {
            let access = CustomerResolvedScope {
                user_id: "actor".into(),
                resource: "customer".into(),
                action: action.into(),
                role_clauses: vec![],
                user_limit: limit.map(|mask| CustomerResolvedClause {
                    company: mask & 1 != 0,
                    self_owned: false,
                    collaborative: false,
                    org_unit_ids: vec![],
                }),
                policy_version: 1,
                organization_version: 1,
                scope_version: "v1".into(),
                as_of: erp_core::common::time::Instant::from_unix_secs(0),
            };
            let history = if action == "detail" { selected(ids, 8) } else { Vec::new() };
            let compiled = customer_scope(&access, "actor", &[], &[], history, &[]);
            let mut expected = BTreeSet::new();
            for (index, id) in ids.iter().enumerate() {
                let object = CustomerScopeObject {
                    owned: index & 1 != 0,
                    collaborating: index & 2 != 0,
                    historical_read_participant: index & 8 != 0,
                    org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                };
                if port.allows(&access, &object).expect("public decision") {
                    expected.insert(id.clone());
                }
            }
            let actual = real_ids(collection, compiled.document()).await?;
            assert_eq!(expected, actual, "customer missing-clauses action={action} limit={limit:?}");
            cases += 1;
        }
    }
    Ok(cases)
}

async fn verify_contract(db: &Database, rbac: &erp_identity::SharedRbacService) -> Outcome<usize> {
    let collection = db.collection::<Document>("s2_a34_contract");
    let ids = fixture_ids();
    let docs = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            doc! {
                "id": id,
                "customer_id": id,
                "owner_user_id": if index & 1 != 0 { "actor" } else { "other" },
                "business_org_unit_id": if index & 4 != 0 { "org-a" } else { "org-b" },
            }
        })
        .collect::<Vec<_>>();
    collection.insert_many(docs).await?;
    let port = MongoContractDataScope::new(db.clone(), rbac.clone());
    let clause = |mask: u8| ContractResolvedClause {
        company: mask & 1 != 0,
        self_owned: mask & 2 != 0,
        collaborative: mask & 4 != 0,
        org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
    };
    let owned = selected(&ids, 1);
    let collaborating = selected(&ids, 2);
    let org_owned = vec![(vec!["org-a".into()], selected(&ids, 4))];
    let mut cases = 0;
    for action in ACTIONS {
        for role in ROLE_MASKS {
            for second in SECOND_MASKS {
                for limit in LIMITS {
                    let access = ContractResolvedScope {
                        user_id: "actor".into(),
                        resource: "contract".into(),
                        action: action.into(),
                        role_clauses: vec![clause(role), clause(second)],
                        user_limit: (limit >= 0).then(|| clause(limit as u8)),
                        policy_version: 1,
                        organization_version: 1,
                        scope_version: "v1".into(),
                        as_of: erp_core::common::time::Instant::from_unix_secs(0),
                    };
                    let mut history = Vec::new();
                    if action == "detail" {
                        history = selected(&ids, 8);
                    }
                    if let Some(limit_clause) = &access.user_limit {
                        let mut upper = access.clone();
                        upper.role_clauses = vec![limit_clause.clone()];
                        upper.user_limit = None;
                        let upper =
                            contract_scope(&upper, "actor", &owned, &collaborating, vec![], &org_owned);
                        history.retain(|id| {
                            upper.authorized_customer_ids.as_ref().is_none_or(|allowed| allowed.contains(id))
                        });
                    }
                    let compiled =
                        contract_scope(&access, "actor", &owned, &collaborating, history, &org_owned);
                    let mut expected = BTreeSet::new();
                    for (index, id) in ids.iter().enumerate() {
                        let object = ContractScopeObject {
                            owned: index & 1 != 0,
                            collaborating: index & 2 != 0,
                            historical_read_participant: index & 8 != 0,
                            org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                        };
                        if port.allows(&access, &object).expect("public decision") {
                            expected.insert(id.clone());
                        }
                    }
                    let actual = real_ids(&collection, compiled.document()).await?;
                    assert_eq!(
                        expected, actual,
                        "contract action={action} role={role} second={second} limit={limit}"
                    );
                    cases += 1;
                }
            }
        }
    }
    Ok(cases)
}

async fn verify_purchase(db: &Database, rbac: &erp_identity::SharedRbacService) -> Outcome<usize> {
    let collection = db.collection::<Document>("s2_a34_purchase");
    let ids = fixture_ids();
    let docs = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            doc! {
                "id": id,
                "customer_id": id,
                "owner_user_id": if index & 1 != 0 { "actor" } else { "other" },
                "business_org_unit_id": if index & 4 != 0 { "org-a" } else { "org-b" },
            }
        })
        .collect::<Vec<_>>();
    collection.insert_many(docs).await?;
    let port = MongoPurchaseDataScope::new(db.clone(), rbac.clone());
    let clause = |mask: u8| PurchaseResolvedClause {
        company: mask & 1 != 0,
        self_owned: mask & 2 != 0,
        collaborative: mask & 4 != 0,
        org_unit_ids: if mask & 8 != 0 { vec!["org-a".into()] } else { vec![] },
    };
    let mut cases = 0;
    for action in ACTIONS {
        for role in ROLE_MASKS {
            for second in SECOND_MASKS {
                for limit in LIMITS {
                    let access = PurchaseResolvedScope {
                        user_id: "actor".into(),
                        resource: "purchase_order".into(),
                        action: action.into(),
                        role_clauses: vec![clause(role), clause(second)],
                        user_limit: (limit >= 0).then(|| clause(limit as u8)),
                        policy_version: 1,
                        organization_version: 1,
                        scope_version: "v1".into(),
                        as_of: erp_core::common::time::Instant::from_unix_secs(0),
                    };
                    let mut history = Vec::new();
                    if action == "detail" {
                        history = selected(&ids, 8);
                    }
                    let compiled = purchase_scope(&access, "actor", history);
                    let mut expected = BTreeSet::new();
                    for (index, id) in ids.iter().enumerate() {
                        let object = PurchaseScopeObject {
                            owned: index & 1 != 0,
                            collaborating: false,
                            historical_read_participant: index & 8 != 0,
                            org_unit_id: Some(if index & 4 != 0 { "org-a" } else { "org-b" }.into()),
                        };
                        if port.allows(&access, &object).expect("public decision") {
                            expected.insert(id.clone());
                        }
                    }
                    let actual = real_ids(&collection, compiled.document()).await?;
                    assert_eq!(
                        expected, actual,
                        "purchase action={action} role={role} second={second} limit={limit}"
                    );
                    cases += 1;
                }
            }
        }
    }
    Ok(cases)
}

async fn verify_sales(db: &Database) -> Outcome<usize> {
    use std::collections::BTreeSet as OrgSet;

    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use erp_sales::entity::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};

    let collection = db.collection::<Document>("s2_a34_sales");
    let ids = fixture_ids();
    let docs = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            doc! {
                "id": id,
                "sales_owner_user_id": if index & 1 != 0 { "actor" } else { "other" },
                "business_org_unit_id": if index & 4 != 0 { "org-a" } else { "org-b" },
                "customer_id": "customer",
            }
        })
        .collect::<Vec<_>>();
    collection.insert_many(docs).await?;

    let clause = |mask: u8| ScopeClause {
        company: mask & 1 != 0,
        self_owned: mask & 2 != 0,
        collaborative: mask & 4 != 0,
        org_unit_ids: if mask & 8 != 0 { OrgSet::from(["org-a".into()]) } else { OrgSet::new() },
        ..Default::default()
    };
    let mut order = SalesOrder::new(
        SalesOrderId::new("order"),
        SalesOrderData {
            sales_owner_user_id: "actor".into(),
            business_org_unit_id: "org-a".into(),
            order_no: "SO-test".into(),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("customer"),
            contract_id: None,
            settlement_party_id: PartyId::new("party"),
            source_status_code: None,
        },
        "creator",
    )
    .map_err(|error| format!("order fixture: {error}"))?;
    let mut cases = 0;
    for action in ACTIONS {
        for role in ROLE_MASKS {
            for second in SECOND_MASKS {
                for limit in LIMITS {
                    for customers in [Vec::<String>::new(), vec!["customer".to_string()]] {
                        let access = AuthorizedDataScope {
                            user_id: "actor".into(),
                            resource: "sales_order".into(),
                            action: action.into(),
                            role_scopes: Default::default(),
                            scope: ResolvedScope {
                                role_clauses: vec![clause(role), clause(second)],
                                user_limit: (limit >= 0).then(|| clause(limit as u8)),
                            },
                            organizations: Default::default(),
                            policy_version: 1,
                            scope_version: "v1".into(),
                            as_of: Instant::from_unix_secs(0),
                        };
                        let history = if action == "detail" { selected(&ids, 8) } else { Vec::new() };
                        let scope = sales_scope(&access, "actor", &customers, history);
                        let mut expected = BTreeSet::new();
                        for (index, id) in ids.iter().enumerate() {
                            order.base.id = id.clone();
                            order.sales_owner_user_id = if index & 1 != 0 { "actor" } else { "other" }.into();
                            order.business_org_unit_id =
                                if index & 4 != 0 { "org-a" } else { "org-b" }.into();
                            if SalesAccess::allows(&access, &scope, &order).expect("public decision") {
                                expected.insert(id.clone());
                            }
                        }
                        let actual = real_ids(&collection, scope.document()).await?;
                        assert_eq!(
                            expected,
                            actual,
                            "sales action={action} role={role} second={second} limit={limit} customers={}",
                            customers.len(),
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    Ok(cases)
}
