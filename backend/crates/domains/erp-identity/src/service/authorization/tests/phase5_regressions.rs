//! 第五阶段缺陷经真实公共入口验证资格、目标、范围与到期拒绝。
use std::sync::Arc;
use std::sync::atomic::Ordering;

use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, cost, customer, fulfillment, internal, inventory, purchase_order, returns, sales_order,
    supplier, supplier_fulfillment,
};
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::fixtures::{Facts, Identity, actor, candidate, complete, inquiry_members, object};
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessTarget, FactValue, ObjectFacts, RelatedObjects, RelationKey};
use crate::entity::resource_access::ResourceKind;
use crate::{AuthorizationService, Error};

// 换入当前用例的角色权限，用于比较同角色组合与分离授权。
fn grant(identity: &Identity, roles: &[(&str, &[Permission])]) {
    identity.state.lock().unwrap().grants =
        roles.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect();
}

// 用指定身份和单个资源事实装配真实服务，供写入与回放用例调用。
fn bound(identity: Arc<Identity>, resource: &str, facts: Arc<Facts>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.facts.insert(resource.into(), facts);
    builder.build().unwrap()
}

// 构造归属于当前用户及指定组织的销售单候选。
fn sales_candidate(org: &str) -> ObjectFacts {
    ObjectFacts { org_unit_id: FactValue::Known(Some(org.into())), ..candidate("sales_order", "user") }
}

// 创建并提交只使用同时具备两项资格的角色范围，候选和持久化回放都应用该限制。
#[tokio::test]
async fn create_and_submit_uses_only_jointly_qualified_roles_for_creation_scope_and_replay() {
    let identity = Arc::new(Identity::default());
    grant(
        &identity,
        &[("creator", &[sales_order::CREATE]), ("joint", &[sales_order::CREATE, sales_order::SUBMIT])],
    );
    identity.role_scopes.lock().unwrap().extend([
        (("creator".into(), sales_order::CREATE), ScopeClause { company: true, ..Default::default() }),
        (
            ("joint".into(), sales_order::CREATE),
            ScopeClause { org_unit_ids: ["org-b".into()].into(), ..Default::default() },
        ),
        (("joint".into(), sales_order::SUBMIT), ScopeClause { company: true, ..Default::default() }),
    ]);
    let persisted = ObjectFacts { id: "order".into(), version: "1".into(), ..sales_candidate("org-a") };
    let facts = Arc::new(Facts::new(vec![persisted]));
    let service = bound(identity, "sales_order", facts.clone());
    let mut executor = SnapshotExecutor::default();
    // 两项普通检查各自允许，不能用这种组合代替创建入口的完整同角色范围。
    for permission in [sales_order::CREATE, sales_order::SUBMIT] {
        service
            .require(&actor(), &permission, AccessTarget::Candidate(sales_candidate("org-a")), &mut executor)
            .await
            .unwrap();
    }

    for org in ["org-a", "org-b"] {
        facts.objects.lock().unwrap()[0].org_unit_id = FactValue::Known(Some(org.into()));
        for target in [AccessTarget::Candidate(sales_candidate(org)), AccessTarget::record("order").unwrap()]
        {
            let result =
                service.require(&actor(), &internal::SALES_CREATE_AND_SUBMIT, target, &mut executor).await;
            if org == "org-b" {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
        }
    }
}

// 额外成本读取需要独立的成本资格，并保留销售单详情的对象范围规则。
#[tokio::test]
async fn extra_cost_requires_independent_cost_qualification_and_preserves_detail_data_rules() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("sales", &[sales_order::DETAIL])]);
    let facts = Arc::new(Facts::new(vec![object("sales_order", "order", "user")]));
    let service = bound(identity.clone(), "sales_order", facts.clone());
    let mut executor = SnapshotExecutor::default();
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::SALES_ORDER_EXTRA_COST_READ,
                AccessTarget::record("order").unwrap(),
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        service.query(&actor(), &internal::SALES_ORDER_EXTRA_COST_READ, &mut executor).await,
        Err(Error::Forbidden(_))
    ));

    grant(&identity, &[("sales", &[sales_order::DETAIL]), ("cost", &[cost::ENTRY_LIST])]);
    let access =
        service.query(&actor(), &internal::SALES_ORDER_EXTRA_COST_READ, &mut executor).await.unwrap();
    assert_eq!(inquiry_members(&access), ["one"]);
    service.validate(&access, &mut executor).await.unwrap();

    identity.scopes.lock().unwrap().insert(
        (internal::SALES_ORDER_EXTRA_COST_READ, sales_order::DETAIL),
        ResolvedScope {
            role_clauses: vec![ScopeClause { org_unit_ids: ["allowed".into()].into(), ..Default::default() }],
            user_limit: None,
        },
    );
    for (org, record_grant, allowed) in
        [("allowed", false, true), ("outside", false, false), ("outside", true, true)]
    {
        facts.objects.lock().unwrap()[0].org_unit_id = FactValue::Known(Some(org.into()));
        if record_grant {
            identity.record_grants.lock().unwrap().insert((ResourceKind::SalesOrder, "order".into()));
        }
        let result = service
            .require(
                &actor(),
                &internal::SALES_ORDER_EXTRA_COST_READ,
                AccessTarget::record("order").unwrap(),
                &mut executor,
            )
            .await;
        if allowed {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
    }

    grant(&identity, &[("sales", &[sales_order::DETAIL])]);
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::SALES_ORDER_EXTRA_COST_READ,
                AccessTarget::record("order").unwrap(),
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));
}

// 各履约创建入口都检查自己的当前权限和账号状态，不复用旧资格。
#[tokio::test]
async fn fulfillment_creation_requires_each_current_functional_grant_and_active_identity() {
    let identity = Arc::new(Identity::default());
    let service = complete_with_identity(identity.clone());
    let mut executor = SnapshotExecutor::default();
    for permission in [
        fulfillment::DELIVERY_DELIVERY_CREATE,
        fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE,
        fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE,
        fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE,
    ] {
        grant(&identity, &[("creator", &[])]);
        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        grant(&identity, &[("creator", std::slice::from_ref(&permission))]);
        service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await.unwrap();
        identity.state.lock().unwrap().active = false;
        assert!(matches!(
            service.require(&actor(), &permission, AccessTarget::Operation, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        identity.state.lock().unwrap().active = true;
    }
}

// 沿用完整测试绑定，只换身份设施来验证不依赖业务事实的入口。
fn complete_with_identity(identity: Arc<Identity>) -> AuthorizationService {
    let mut builder = complete();
    builder.facilities = identity;
    builder.build().unwrap()
}

// 采购退货的真实来源沿用完整命令角色，无需采购单详情菜单资格。
#[tokio::test]
async fn purchase_return_source_requires_the_complete_command_role_without_a_detail_menu() {
    let identity = Arc::new(Identity::default());
    let facts = Arc::new(Facts::new(vec![object("purchase_order", "purchase", "other")]));
    let service = bound(identity.clone(), "purchase_order", facts.clone());
    let permission = &internal::PURCHASE_RETURN_SOURCE;
    identity.scopes.lock().unwrap().insert(
        (permission.clone(), permission.clone()),
        ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        },
    );
    let required = [purchase_order::SAVE_DRAFT, returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE];
    let detail_only = [purchase_order::DETAIL];
    let mut executor = SnapshotExecutor::default();
    grant(&identity, &[("operator", &required)]);
    service
        .require(&actor(), permission, AccessTarget::record("purchase").unwrap(), &mut executor)
        .await
        .expect("固定采购退货来源使用已取证的公司范围，完整命令角色无需详情菜单资格");
    assert_eq!(facts.calls.load(Ordering::SeqCst), 1);

    for roles in [
        vec![("operator", &required[..1])],
        vec![("operator", &required[1..])],
        vec![("editor", &required[..1]), ("creator", &required[1..])],
        vec![("reader", detail_only.as_slice())],
    ] {
        grant(&identity, &roles);
        let calls = facts.calls.load(Ordering::SeqCst);
        assert!(matches!(
            service
                .require(&actor(), permission, AccessTarget::record("purchase").unwrap(), &mut executor)
                .await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(facts.calls.load(Ordering::SeqCst), calls);
    }

    grant(&identity, &[("operator", &required)]);
    assert!(matches!(
        service
            .require(
                &actor(),
                permission,
                AccessTarget::record("purchase").unwrap(),
                &mut ReadExecutor::default()
            )
            .await,
        Err(Error::ValidationError(_))
    ));
}

// 库存调整更新检查已持久化的调整单和完整仓库来源，不接受候选替代。
#[tokio::test]
async fn stock_adjustment_update_checks_the_persisted_record_and_complete_warehouse_source() {
    let identity = Arc::new(Identity::default());
    let permission = &inventory::STOCK_ADJUSTMENT_UPDATE;
    let required = [inventory::STOCK_ADJUSTMENT_UPDATE, inventory::STOCK_ADJUSTMENT_DETAIL];
    grant(&identity, &[("warehouse-editor", &required)]);
    identity.scopes.lock().unwrap().insert(
        (permission.clone(), permission.clone()),
        ResolvedScope {
            role_clauses: vec![ScopeClause {
                warehouse_ids: ["warehouse".into()].into(),
                ..Default::default()
            }],
            user_limit: None,
        },
    );

    let warehouse_relation = RelationKey::parse("warehouse").unwrap();
    let adjustment = ObjectFacts {
        warehouse_id: FactValue::Known(Some("warehouse".into())),
        relations: [(
            warehouse_relation.clone(),
            RelatedObjects {
                expected_ids: ["warehouse".into()].into(),
                objects: vec![object("warehouse", "warehouse", "other")],
            },
        )]
        .into(),
        ..object("stock_adjustment", "adjustment", "other")
    };
    let facts = Arc::new(Facts::new(vec![adjustment.clone()]));
    let service = bound(identity.clone(), "stock_adjustment", facts.clone());
    let mut executor = SnapshotExecutor::default();
    let target = || AccessTarget::record("adjustment").unwrap();
    service
        .require(&actor(), permission, target(), &mut executor)
        .await
        .expect("库存调整更新与详情必须来自同一角色，真实仓库在该角色范围内");

    for roles in [
        vec![("update-only", &required[..1])],
        vec![("detail-only", &required[1..])],
        vec![("editor", &required[..1]), ("reader", &required[1..])],
    ] {
        grant(&identity, &roles);
        let calls = facts.calls.load(Ordering::SeqCst);
        assert!(matches!(
            service.require(&actor(), permission, target(), &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        assert_eq!(facts.calls.load(Ordering::SeqCst), calls);
    }

    grant(&identity, &[("warehouse-editor", &required)]);
    facts.objects.lock().unwrap()[0].warehouse_id = FactValue::Known(Some("outside".into()));
    assert!(matches!(
        service.require(&actor(), permission, target(), &mut executor).await,
        Err(Error::Forbidden(_))
    ));

    *facts.objects.lock().unwrap() = vec![adjustment.clone()];
    facts.objects.lock().unwrap()[0].relations.get_mut(&warehouse_relation).unwrap().objects.clear();
    assert!(matches!(
        service.require(&actor(), permission, target(), &mut executor).await,
        Err(Error::Forbidden(_))
    ));

    *facts.objects.lock().unwrap() = vec![adjustment];
    facts.objects.lock().unwrap()[0].warehouse_id = FactValue::Missing;
    assert!(matches!(
        service.require(&actor(), permission, target(), &mut executor).await,
        Err(Error::ValidationError(message)) if message.contains("所属仓库")
    ));

    assert!(matches!(
        service.require(&actor(), permission, AccessTarget::record("missing").unwrap(), &mut executor).await,
        Err(Error::NotFound(_))
    ));
    let calls = facts.calls.load(Ordering::SeqCst);
    assert!(matches!(
        service
            .require(
                &actor(),
                permission,
                AccessTarget::Candidate(candidate("stock_adjustment", "user")),
                &mut executor
            )
            .await,
        Err(Error::ValidationError(message)) if message == "此权限不支持该授权目标"
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), calls);
}

// 登记供应商导入必须重新检查当前导入资格。
#[tokio::test]
async fn supplier_import_registration_requires_current_import_qualification() {
    let identity = Arc::new(Identity::default());
    let service = complete_with_identity(identity.clone());
    let mut executor = SnapshotExecutor::default();
    for (active, permissions, allowed) in [
        (true, vec![], false),
        (true, vec![supplier::IMPORT_SUPPLIER_IMPORT], true),
        (false, vec![supplier::IMPORT_SUPPLIER_IMPORT], false),
    ] {
        grant(&identity, &[("importer", &permissions)]);
        identity.state.lock().unwrap().active = active;
        let result = service
            .require(&actor(), &internal::SUPPLIER_IMPORT_SUBMIT, AccessTarget::Operation, &mut executor)
            .await;
        if allowed {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
    }
}

// 供应商补全回放要求同一角色持有全部三项现行权限。
#[tokio::test]
async fn supplier_completion_replay_requires_one_role_with_all_three_current_permissions() {
    let identity = Arc::new(Identity::default());
    let service = complete_with_identity(identity.clone());
    let mut executor = SnapshotExecutor::default();
    let required = [
        supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
        supplier_fulfillment::ORDER_INVESTIGATION,
        supplier_fulfillment::ORDER_TASK_COMPLETION,
    ];
    grant(
        &identity,
        &[("setup", &required[..1]), ("investigate", &required[1..2]), ("complete", &required[2..])],
    );
    assert!(matches!(
        service
            .require(
                &actor(),
                &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
                AccessTarget::Operation,
                &mut executor,
            )
            .await,
        Err(Error::Forbidden(_))
    ));

    grant(&identity, &[("operator", &required)]);
    service
        .require(
            &actor(),
            &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
            AccessTarget::Operation,
            &mut executor,
        )
        .await
        .unwrap();

    for (active, permissions) in [(true, &required[..2]), (false, required.as_slice())] {
        grant(&identity, &[("operator", permissions)]);
        identity.state.lock().unwrap().active = active;
        assert!(matches!(
            service
                .require(
                    &actor(),
                    &internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
                    AccessTarget::Operation,
                    &mut executor,
                )
                .await,
            Err(Error::Forbidden(_))
        ));
    }
}

// 客户创建回包按当前创建资格和范围复查真实记录，不放过不存在的对象。
#[tokio::test]
async fn customer_creation_receipt_rechecks_the_real_record_with_only_current_creation_scope() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("creator", &[customer::CREATE])]);
    let facts = Arc::new(Facts::new(vec![object("customer", "customer", "user")]));
    let service = bound(identity.clone(), "customer", facts.clone());
    let mut executor = SnapshotExecutor::default();
    let permission = &internal::CUSTOMER_CREATED_RESULT_READ;
    service
        .require(&actor(), permission, AccessTarget::record("customer").unwrap(), &mut executor)
        .await
        .unwrap();
    for target in [AccessTarget::Operation, AccessTarget::Candidate(candidate("customer", "user"))] {
        assert!(matches!(
            service.require(&actor(), permission, target, &mut executor).await,
            Err(Error::ValidationError(_))
        ));
    }
    {
        let mut objects = facts.objects.lock().unwrap();
        objects[0].owner_id = FactValue::Known(Some("other".into()));
        objects[0].history_participants = FactValue::Known(["user".into()].into());
    }
    assert!(matches!(
        service.require(&actor(), permission, AccessTarget::record("customer").unwrap(), &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    assert!(matches!(
        service.require(&actor(), permission, AccessTarget::record("missing").unwrap(), &mut executor).await,
        Err(Error::NotFound(_))
    ));

    grant(&identity, &[("reader", &[customer::DETAIL])]);
    let calls = facts.calls.load(Ordering::SeqCst);
    assert!(matches!(
        service.require(&actor(), permission, AccessTarget::record("customer").unwrap(), &mut executor).await,
        Err(Error::Forbidden(_))
    ));
    assert_eq!(facts.calls.load(Ordering::SeqCst), calls);
}

// 客户创建接受归属候选，不能改用记录目标，归属变更授权到期也不放行。
#[tokio::test]
async fn customer_creation_accepts_candidate_and_rejects_record_and_expired_owner_change() {
    let identity = Arc::new(Identity::default());
    grant(&identity, &[("customer", &[customer::CREATE, customer::UPDATE])]);
    let facts = Arc::new(Facts::new(vec![object("customer", "customer", "user")]));
    let service = bound(identity, "customer", facts);
    let mut executor = SnapshotExecutor::default();
    service
        .require(
            &actor(),
            &customer::CREATE,
            AccessTarget::Candidate(candidate("customer", "user")),
            &mut executor,
        )
        .await
        .unwrap();
    assert!(matches!(
        service
            .require(&actor(), &customer::CREATE, AccessTarget::record("customer").unwrap(), &mut executor)
            .await,
        Err(Error::ValidationError(_))
    ));
    for (expiry, expired) in [(None, false), (Some(Instant::from_unix_secs(1)), true)] {
        let destination = ObjectFacts { valid_until: expiry, ..candidate("customer", "user") };
        let result = service
            .require(
                &actor(),
                &customer::UPDATE,
                AccessTarget::OwnershipChange { id: "customer".into(), candidate: destination },
                &mut executor,
            )
            .await;
        if expired {
            assert!(matches!(result, Err(Error::ConflictError(_))));
        } else {
            result.unwrap();
        }
    }
}
