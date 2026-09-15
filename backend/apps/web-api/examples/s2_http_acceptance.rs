//! S2 隔离账号 HTTP 验收；启动真实环回监听，使用随机测试数据库，不调用外部连接器。
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use config::{Config, SafeConfig};
use erp_identity::access_control::{
    DataScopeData, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension,
};
use erp_identity::{LoginAccount, Secret};
use mongodb::{
    bson::{doc, Document},
    Client, Database,
};
use serde_json::{json, Value};
use storage::{S3Storage, S3StorageConfig};
use test_support::seed_admin_account;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use web_api::{app_state::AppState, core::routes};

type Outcome<T = ()> = Result<T, Box<dyn Error>>;

#[tokio::main]
async fn main() -> Outcome {
    let uri = std::env::var("ERP_TEST_MONGO_URI")?;
    let client = Client::with_uri_str(&uri).await?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let db = client.database(&format!("erp_s2_http_{suffix}"));
    let result = verify(&db, &uri, &format!("s2-isolated-{suffix}-http-key")).await;
    db.drop().await?;
    result
}

async fn verify(db: &Database, uri: &str, secret: &str) -> Outcome {
    erp_identity::indexes::ensure(db).await?;
    let user = seed_admin_account(db).await?;
    let role = permissions(db, &user, true).await?;
    let login = "s2_http_acceptance";
    let password = "S2-isolated-fixture-password";
    let credentials =
        mongodb::bson::serialize_to_document(&Secret::new(LoginAccount::new(login)?, password)?)?;
    db.collection::<Document>("accounts")
        .update_one(doc! { "id": &user }, doc! { "$set": credentials })
        .await?;
    let restricted = seed_admin_account(db).await?;
    permissions(db, &restricted, false).await?;
    let other_credentials = mongodb::bson::serialize_to_document(&Secret::new(
        LoginAccount::new("s2_scope_missing")?,
        password,
    )?)?;
    db.collection::<Document>("accounts")
        .update_one(doc! { "id": &restricted }, doc! { "$set": other_credentials })
        .await?;
    let config: Config = serde_json::from_value(json!({
        "app": {"port":0,"secret":secret}, "database":{"uri":uri,"db_name":db.name()},
        "s3":{"bucket":"s2-acceptance","region":"local","endpoint":"http://127.0.0.1:1",
        "access_key_id":"isolated","secret_access_key":"isolated","public_base_url":"http://127.0.0.1:1"}
    }))?;
    let s3 = S3Storage::new(S3StorageConfig {
        bucket: "s2-acceptance".into(),
        region: "local".into(),
        endpoint: Some("http://127.0.0.1:1".into()),
        access_key_id: "isolated".into(),
        secret_access_key: "isolated".into(),
        session_token: None,
        key_prefix: None,
        public_base_url: "http://127.0.0.1:1".into(),
        force_path_style: true,
    })?;
    let state = AppState::new(db.clone(), SafeConfig::new(config), s3);
    state
        .rbac()
        .seed_data_scope_manifest(
            &role,
            "org_unit",
            vec![DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.clone(),
                scope_type: DataScopeType::Company,
                scope_targets: vec![],
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: "org_unit".into(),
                    actions: vec!["list".into(), "manage".into()],
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: None,
                    include_descendants: None,
                    enabled: true,
                },
            }],
        )
        .await?;
    for (resource, actions) in [
        ("sales_order", vec!["detail".to_string()]),
        ("work_item", vec!["manage".to_string()]),
    ] {
        state
            .rbac()
            .seed_data_scope_manifest(
                &role,
                resource,
                vec![DataScopeData {
                    subject_type: DataScopeSubjectType::Role,
                    subject_id: role.clone(),
                    scope_type: DataScopeType::Company,
                    scope_targets: vec![],
                    binding: ScopeBinding {
                        schema_version: 2,
                        resource: resource.into(),
                        actions,
                        target_dimension: ScopeDimension::InternalOrg,
                        target_mode: None,
                        include_descendants: None,
                        enabled: true,
                    },
                }],
            )
            .await?;
    }
    seed_task_orders(db, &user).await?;
    verify_binding_source(db, state.rbac().clone(), &user, &restricted).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?.to_string();
    let router = routes::create(state);
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
    });
    let result = async {
        let (status, _) = request(&address, "POST", "/login", None, Some(json!({"account":login,"password":"wrong-password","account_kind":"admin"}))).await?;
        assert_eq!(status, 401, "wrong password must fail");
        let (status, authentication) = request(&address, "POST", "/login", None, Some(json!({"account":login,"password":password,"account_kind":"admin"}))).await?;
        assert_eq!(status, 200, "real password login");
        let token = authentication["data"]["token"].as_str().ok_or("login token missing")?;
        let (status, other) = request(&address, "POST", "/login", None, Some(json!({"account":"s2_scope_missing","password":password,"account_kind":"admin"}))).await?;
        assert_eq!(status, 200);
        let other_token = other["data"]["token"].as_str().ok_or("second login missing token")?;
        let (status, mine) = request(&address, "GET", "/admin/work-items?scope=mine", Some(other_token), None).await?;
        assert_eq!(status, 200);
        assert_eq!(mine["data"]["total"], 0);
        let (status, _) = request(&address, "GET", "/admin/work-items?scope=managed", Some(other_token), None).await?;
        assert_eq!(status, 403, "manage permission without scope must not become Company");
        requests(&address, token).await?;
        let (status, _) = request(&address, "POST", "/admin/work-items/s2-task-0/reassign", Some(token), Some(json!({"expected_task_version":"1","target_user_id":restricted,"reason":"S2 candidate negative acceptance","idempotency_key":"s2-denied-reassign"}))).await?;
        assert!(matches!(status, 403 | 422), "unqualified recipient must be refused, got {status}");
        let task = db.collection::<Document>("work_items").find_one(doc! {"id":"s2-task-0"}).await?.ok_or("task disappeared")?;
        assert_eq!(task.get_str("owner_user_id")?, user);
        println!("PASS real_http_two_accounts_missing_scope_and_unqualified_reassignment_no_write");
        Ok(())
    }.await;
    server.abort();
    let _ = server.await;
    result
}

async fn requests(address: &str, token: &str) -> Outcome {
    let path = "/admin/work-items?scope=mine&page_size=1";
    assert_eq!(request(address, "GET", path, None, None).await?.0, 401);
    let (status, page) = request(address, "GET", path, Some(token), None).await?;
    assert_eq!(status, 200, "authenticated first page: {page}");
    assert_eq!(page["data"]["total"], 2);
    assert_eq!(page["data"]["items"].as_array().ok_or("items missing")?.len(), 1);
    let version = page["data"]["scope_version"]
        .as_str()
        .ok_or("scope_version missing")?;
    let (status, error) = request(address, "GET", &format!("{path}&page=2"), Some(token), None).await?;
    assert_eq!(status, 409);
    assert_eq!(error["code"], "DATA_SCOPE_CHANGED");
    let next = format!("{path}&page=2&scope_version={version}");
    assert_eq!(request(address, "GET", &next, Some(token), None).await?.0, 200);
    let change = json!({"expected_version":0,"idempotency_key":"http_create_department","reason":"S2 isolated HTTP acceptance",
        "change":{"operation":"create_unit","name":"HTTP验收部门","parent_id":null,"kind":"department"}});
    let (status, result) = request(
        address,
        "POST",
        "/admin/org-units/change",
        Some(token),
        Some(change),
    )
    .await?;
    assert_eq!(status, 200, "organization HTTP change: {result}");
    let (status, error) = request(address, "GET", &next, Some(token), None).await?;
    assert_eq!(status, 409, "old queue scope must fail: {error}");
    assert_eq!(error["code"], "DATA_SCOPE_CHANGED");
    let (status, refreshed) = request(address, "GET", path, Some(token), None).await?;
    assert_eq!(status, 200);
    assert_ne!(refreshed["data"]["scope_version"], version);
    println!(
        "PASS real_http_authentication_first_page_missing_anchor_stable_anchor_organization_change_refresh"
    );
    println!("LIMIT isolated fixture accounts and draft order tasks; existing business accounts and end-to-end approval acceptance remain separate");
    Ok(())
}

async fn request(
    address: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Outcome<(u16, Value)> {
    let body = body.map(|value| value.to_string()).unwrap_or_default();
    let mut bytes = format!("{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", body.len());
    if let Some(token) = token {
        bytes.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    bytes.push_str("\r\n");
    bytes.push_str(&body);
    let mut stream = TcpStream::connect(address).await?;
    stream.write_all(bytes.as_bytes()).await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    let response = String::from_utf8(response)?;
    let (headers, body) = response.split_once("\r\n\r\n").ok_or("malformed HTTP response")?;
    let status = headers
        .split_whitespace()
        .nth(1)
        .ok_or("HTTP status missing")?
        .parse()?;
    Ok((status, serde_json::from_str(body)?))
}

async fn permissions(db: &Database, user: &str, full: bool) -> Outcome<String> {
    let rule = db
        .collection::<Document>("casbin_rules")
        .find_one(doc! {"ptype":"g","values.0":format!("user:admin:{user}")})
        .await?
        .ok_or("role missing")?;
    let key = rule.get_array("values")?[1].as_str().ok_or("invalid role")?;
    let grants: &[(&str, &str)] = if full {
        &[
            ("work_item", "list"),
            ("work_item", "detail"),
            ("work_item", "manage"),
            ("work_item", "reassign"),
            ("sales_order", "detail"),
            ("org_unit", "list"),
            ("org_unit", "manage"),
        ]
    } else {
        &[("work_item", "list"), ("work_item", "manage")]
    };
    for &(resource, action) in grants {
        db.collection::<Document>("casbin_rules")
            .insert_one(
                doc! {"_id":format!("p\u{1f}p\u{1f}{key}\u{1f}{resource}\u{1f}{action}"),
                "sec":"p","ptype":"p","values":[key,resource,action]},
            )
            .await?;
    }
    Ok(key.strip_prefix("role:").ok_or("invalid role")?.into())
}

async fn seed_task_orders(db: &Database, owner: &str) -> Outcome {
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId, WorkItemId};
    use erp_sales::entity::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};
    use erp_workflow::entity::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };
    for index in 0..2 {
        let id = format!("s2-order-{index}");
        let order = SalesOrder::new(
            SalesOrderId::new(&id),
            SalesOrderData {
                sales_owner_user_id: owner.into(),
                business_org_unit_id: "fixture-dept".into(),
                order_no: format!("S2-{index}"),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("s2-customer"),
                contract_id: None,
                settlement_party_id: PartyId::new("s2-party"),
                source_status_code: None,
            },
            owner,
        )?;
        db.collection::<SalesOrder>("sales_orders")
            .insert_one(order)
            .await?;
        let task = WorkItem::new_with_responsibility_key(
            WorkItemId::new(format!("s2-task-{index}")),
            WorkItemData {
                work_item_type: WorkItemType::CustomerAcceptanceRegistration,
                business_object_type: "sales_order".into(),
                business_object_id: id.clone(),
                subject_version: "1".into(),
                owner_role: "sales_order_owner".into(),
                owner_organization_id: "s2-party".into(),
                owner_user_id: owner.into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("CUSTOMER_ACCEPTANCE_REQUIRED".into()),
                impact_summary: None,
            },
            format!("sales_order:{id}:customer_acceptance"),
        )?;
        db.collection::<WorkItem>("work_items").insert_one(task).await?;
    }
    Ok(())
}

/// 使用与 HTTP 相同数据库和账号核验审批绑定的独立订单读取端口。
async fn verify_binding_source(
    db: &Database,
    rbac: erp_identity::SharedRbacService,
    owner: &str,
    restricted: &str,
) -> Outcome {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_processes::adapters::workflow::WorkflowAuth;
    use erp_workflow::ports::WorkflowAuthorizationPort;
    use erp_workflow::DocumentType;
    use persistence_core::NoTransaction;
    let auth = WorkflowAuth::new(db.clone(), rbac);
    let object = auth
        .approval_scope_object(DocumentType::SalesOrder, "s2-order-0", &mut NoTransaction)
        .await?;
    assert_eq!(object.business_org_unit_id.as_deref(), Some("fixture-dept"));
    assert_eq!(object.owner_user_id, owner);
    assert!(object.settlement_party_id.is_none() && object.warehouse_id.is_none());
    assert!(
        auth.binding_order_readable(
            &AuditActor::new(owner.into(), owner.into(), AccountKind::Admin),
            &object,
            &mut NoTransaction
        )
        .await?
    );
    assert!(
        !auth
            .binding_order_readable(
                &AuditActor::new(restricted.into(), restricted.into(), AccountKind::Admin),
                &object,
                &mut NoTransaction
            )
            .await?
    );
    println!("PASS real_binding_source_internal_org_and_independent_order_read");
    Ok(())
}
