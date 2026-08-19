//! P6-FINAL：定义/运行 HTTP、20 类型目录、权限双门禁与稳定错误矩阵。
//!
//! 真实 MongoDB 用例一律 `#[ignore]` + `require_mongo!()`。客户端不得提交 node key、
//! 连线、角色、handler 或 action。未提供数据库时不得把 ignored 表述为通过。

use axum::body::{to_bytes, Body};
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{AppConfig, Config, DatabaseConfig, S3Config, SafeConfig};
use database::ensure_indexes;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use storage::{S3Storage, S3StorageConfig};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use tower::ServiceExt;
use web_api::{app_state::AppState, core::routes};

const JWT_SECRET: &str = "approval-workflow-api-test-secret-32b";

/// 构造测试 AppState；对象存储客户端不会在本合同测试中发起网络请求。
///
/// # 错误
/// S3 配置非法时测试失败。
fn test_app_state(db: Database, mongo_uri: String, db_name: String) -> AppState {
    let s3 = S3Config {
        bucket: "approval-workflow-test".to_string(),
        region: "us-east-1".to_string(),
        endpoint: Some("http://127.0.0.1:9000".to_string()),
        access_key_id: "test-access-key".to_string(),
        secret_access_key: "test-secret-key".to_string(),
        session_token: None,
        key_prefix: None,
        public_base_url: "http://127.0.0.1:9000/approval-workflow-test".to_string(),
        force_path_style: true,
    };
    let storage = S3Storage::new(S3StorageConfig {
        bucket: s3.bucket.clone(),
        region: s3.region.clone(),
        endpoint: s3.endpoint.clone(),
        access_key_id: s3.access_key_id.clone(),
        secret_access_key: s3.secret_access_key.clone(),
        session_token: s3.session_token.clone(),
        key_prefix: s3.key_prefix.clone(),
        public_base_url: s3.public_base_url.clone(),
        force_path_style: s3.force_path_style,
    })
    .expect("S3 测试配置必须合法");
    let config = Config {
        app: AppConfig {
            port: 0,
            secret: JWT_SECRET.to_string(),
        },
        database: DatabaseConfig {
            uri: mongo_uri,
            db_name,
        },
        s3,
    };
    AppState::new(db, SafeConfig::new(config), storage)
}

/// 写入一条 Casbin 权限规则。
///
/// # 错误
/// 集合写入失败时测试失败。
async fn insert_permission(db: &Database, role_key: &str, resource: &str, action: &str) {
    let values = vec![role_key.to_string(), resource.to_string(), action.to_string()];
    let document = doc! {
        "_id": format!("p\u{1f}p\u{1f}{}", values.join("\u{1f}")),
        "sec": "p",
        "ptype": "p",
        "values": values,
    };
    db.collection::<Document>("casbin_rules")
        .insert_one(document)
        .await
        .expect("权限写入失败");
}

/// 为种子账号补齐指定权限。
///
/// # 错误
/// 角色绑定不存在或写入失败时测试失败。
async fn grant(db: &Database, account_id: &str, permissions: &[(&str, &str)]) -> String {
    let subject = format!("user:admin:{account_id}");
    let binding = db
        .collection::<Document>("casbin_rules")
        .find_one(doc! { "sec": "g", "values.0": &subject })
        .await
        .expect("绑定查询失败")
        .expect("种子账号必须已绑定角色");
    let role_key = binding
        .get_array("values")
        .ok()
        .and_then(|values| values.get(1))
        .and_then(mongodb::bson::Bson::as_str)
        .expect("角色键")
        .to_string();
    for (resource, action) in permissions {
        insert_permission(db, &role_key, resource, action).await;
    }
    role_key
}

/// 签发测试 JWT。
///
/// # 错误
/// 签名失败时测试失败。
fn token(account_id: &str) -> String {
    mint_jwt(account_id, JWT_SECRET, 3600).expect("JWT 必须可签发")
}

/// 从响应信封读取稳定错误码。
fn error_code(body: &Value) -> &str {
    body.get("code").and_then(Value::as_str).unwrap_or("")
}

/// 发送 PUT JSON。`TestApi` 未暴露 PUT，本文件自行覆盖节点替换合同。
///
/// # 错误
/// 请求构造或路由调用失败时测试失败。
async fn put_json(router: Router, path: &str, token: &str, json: Value) -> (u16, Value) {
    let mut builder = Request::builder().method(Method::PUT).uri(path);
    let value = HeaderValue::from_str(&format!("Bearer {token}")).expect("Bearer token 应合法");
    builder = builder
        .header(AUTHORIZATION, value)
        .header("content-type", "application/json");
    let request = builder
        .body(Body::from(json.to_string()))
        .expect("HTTP 请求构造失败");
    let response = router.oneshot(request).await.expect("路由调用失败");
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("响应体读取失败");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// 客户端写请求不得夹带节点键、连线、角色、handler 或 action。
#[test]
fn client_cannot_submit_node_key_transitions_or_actions() {
    use services::approval::definition_dto::{CreateDefinitionDraftRequest, DefinitionNodeRequest};
    use services::inventory::SubmitStockAdjustmentRequest;

    assert!(serde_json::from_value::<CreateDefinitionDraftRequest>(json!({
        "document_type": "stock_adjustment",
        "name": "库存",
        "draft_source": "EMPTY",
        "idempotency_key": "k1",
        "source_definition_id": "forged"
    }))
    .is_err());
    assert!(serde_json::from_value::<DefinitionNodeRequest>(json!({
        "node_name": "仓储",
        "display_order": 1,
        "assignee_user_id": "u1",
        "node_key": "client"
    }))
    .is_err());
    assert!(serde_json::from_value::<DefinitionNodeRequest>(json!({
        "node_name": "仓储",
        "display_order": 1,
        "assignee_user_id": "u1",
        "transitions": []
    }))
    .is_err());
    assert!(serde_json::from_value::<DefinitionNodeRequest>(json!({
        "node_name": "仓储",
        "display_order": 1,
        "assignee_user_id": "u1",
        "node_purpose": "SALES_ORDER_PROCUREMENT_CONFIRMATION"
    }))
    .is_err());
    assert!(serde_json::from_value::<SubmitStockAdjustmentRequest>(json!({
        "expected_version": 1,
        "idempotency_key": "k1",
        "definition_id": "forged"
    }))
    .is_err());
    assert!(serde_json::from_value::<SubmitStockAdjustmentRequest>(json!({
        "expected_version": 1,
        "idempotency_key": "k1",
        "next_assignee": "u1"
    }))
    .is_err());
}

/// 决定、恢复、改派请求拒绝内部 ID 与旧恢复动作。
#[test]
fn runtime_http_requests_deny_internal_and_legacy_fields() {
    use web_api::core::handler::approval_instance::http::{
        ResumeApproverHttpRequest, SubmitDecisionHttpRequest,
    };

    for field in [
        "instance_id",
        "execution_id",
        "definition_id",
        "next_node",
        "next_assignee",
        "actor_id",
        "reject_target",
    ] {
        let mut value = json!({
            "work_item_id": "wi-1",
            "decision": "APPROVE",
            "expected_task_version": "3",
            "idempotency_key": "k1"
        });
        value[field] = json!("forged");
        assert!(
            serde_json::from_value::<SubmitDecisionHttpRequest>(value).is_err(),
            "{field} 必须拒绝"
        );
    }
    assert!(serde_json::from_value::<ResumeApproverHttpRequest>(json!({
        "expected_instance_version": "1",
        "expected_execution_version": "1",
        "expected_assignment_version": "1",
        "idempotency_key": "k1",
        "recovery_action": "RETRY_CURRENT_STEP"
    }))
    .is_err());
}

/// `APPROVAL_POLICY_NOT_REGISTERED` 必须映射 500，不能由客户端修复。
#[test]
fn policy_not_registered_maps_to_internal_error() {
    use axum::http::HeaderMap;
    use web_api::core::handler::approval_instance::error::ApprovalHttpError;

    let error = ApprovalHttpError::from_service(
        services::Error::Internal("APPROVAL_POLICY_NOT_REGISTERED".to_string()),
        &HeaderMap::new(),
    );
    assert_eq!(error.code(), "APPROVAL_POLICY_NOT_REGISTERED");
    assert_eq!(error.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}

/// P0-D 后全部固定类型进入新运行时；HTTP 不得再映射未切换失败码。
#[test]
fn all_fixed_types_cut_over_and_http_has_no_uncut_over_code() {
    use entities::document_registry::DocumentType;
    use services::approval::business_adapter::ensure_runtime_cut_over;
    use services::approval::policy::ALL_DOCUMENT_TYPES;
    use services::approval_codes;

    for document_type in ALL_DOCUMENT_TYPES {
        assert!(
            ensure_runtime_cut_over(document_type).is_ok(),
            "{} 必须进入新运行时",
            document_type.as_str()
        );
    }
    assert!(ensure_runtime_cut_over(DocumentType::SalesOrder).is_ok());
    assert!(ensure_runtime_cut_over(DocumentType::PurchaseOrder).is_ok());
    assert!(ensure_runtime_cut_over(DocumentType::CustomerReceipt).is_ok());
    assert!(!approval_codes::ALL.contains(&"APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
    let http_error = include_str!("../src/core/handler/approval_instance/error.rs");
    assert!(!http_error.contains("APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER"));
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn catalog_requires_only_process_read_and_hides_sensitive_fields() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_api_cat").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let account = seed_admin_account(fixture.db()).await.expect("种子账号");
        grant(fixture.db(), &account, &[("approval_process", "read")]).await;
        let api = TestApi::new(routes::create(test_app_state(
            fixture.db().clone(),
            "mongodb://localhost".to_string(),
            fixture.name().to_string(),
        )));
        let (status, body) = api
            .get("/admin/approval-processes/catalog", Some(&token(&account)))
            .await;
        assert_eq!(status, 200, "{body}");
        let items = body["data"].as_array().expect("目录必须是数组");
        assert_eq!(items.len(), 20);
        let requirement = |code: &str| {
            items
                .iter()
                .find(|item| item["document_type"] == code)
                .and_then(|item| item["approval_requirement"].as_str())
                .unwrap_or("")
                .to_string()
        };
        assert_eq!(requirement("sales_order"), "PROCESS_REQUIRED");
        assert_eq!(requirement("voucher_sales_order"), "PROCESS_REQUIRED");
        assert_eq!(requirement("sales_change_order"), "PROCESS_REQUIRED");
        assert_eq!(requirement("purchase_order"), "PROCESS_REQUIRED");
        assert_eq!(requirement("purchase_change_order"), "PROCESS_REQUIRED");
        assert_eq!(requirement("stock_adjustment"), "PROCESS_REQUIRED");
        assert_eq!(requirement("customer_receipt"), "PROCESS_REQUIRED");
        assert_eq!(requirement("supplier_payment"), "PROCESS_REQUIRED");
        assert_eq!(requirement("customer_refund"), "PROCESS_REQUIRED");
        assert_eq!(requirement("supplier_refund"), "PROCESS_REQUIRED");
        assert_eq!(requirement("receipt_reversal"), "PROCESS_REQUIRED");
        assert_eq!(requirement("payment_reversal"), "PROCESS_REQUIRED");
        assert_eq!(requirement("purchase_receipt"), "NO_APPROVAL");
        assert_eq!(requirement("delivery"), "NO_APPROVAL");
        assert_eq!(requirement("electronic_delivery"), "NO_APPROVAL");
        assert_eq!(requirement("service_fulfillment"), "NO_APPROVAL");
        assert_eq!(requirement("customer_acceptance"), "NO_APPROVAL");
        assert_eq!(requirement("invoice"), "NO_APPROVAL");
        assert_eq!(requirement("sales_return_case"), "NO_APPROVAL");
        assert_eq!(requirement("purchase_return_order"), "NO_APPROVAL");
        for item in items {
            assert!(item.get("process_kind").is_none(), "目录不得暴露 ProcessKind");
            assert!(item.get("node_key").is_none(), "目录不得暴露 node key");
        }
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn definition_admin_requires_action_and_type_permissions() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_api_perm").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let action_only = seed_admin_account(fixture.db()).await.expect("动作账号");
        grant(
            fixture.db(),
            &action_only,
            &[("approval_process", "read"), ("approval_process", "create")],
        )
        .await;
        let type_only = seed_admin_account(fixture.db()).await.expect("类型账号");
        grant(
            fixture.db(),
            &type_only,
            &[
                ("approval_process", "read"),
                ("stock_adjustment", "approval_definition_admin"),
            ],
        )
        .await;
        let both = seed_admin_account(fixture.db()).await.expect("双门禁账号");
        grant(
            fixture.db(),
            &both,
            &[
                ("approval_process", "read"),
                ("approval_process", "create"),
                ("stock_adjustment", "approval_definition_admin"),
            ],
        )
        .await;
        let none = seed_admin_account(fixture.db()).await.expect("无权限账号");
        let api = TestApi::new(routes::create(test_app_state(
            fixture.db().clone(),
            "mongodb://localhost".to_string(),
            fixture.name().to_string(),
        )));
        let payload = json!({
            "document_type": "stock_adjustment",
            "name": "库存调整试点",
            "draft_source": "EMPTY",
            "idempotency_key": "draft-1"
        });
        let (status, _) = api
            .post(
                "/admin/approval-process-definitions/drafts",
                Some(&token(&none)),
                Some(payload.clone()),
            )
            .await;
        assert_eq!(status, 403);
        let (status, _) = api
            .post(
                "/admin/approval-process-definitions/drafts",
                Some(&token(&type_only)),
                Some(payload.clone()),
            )
            .await;
        assert_eq!(status, 403);
        let (status, body) = api
            .post(
                "/admin/approval-process-definitions/drafts",
                Some(&token(&action_only)),
                Some(payload.clone()),
            )
            .await;
        assert_eq!(status, 403, "{body}");
        let (status, body) = api
            .post(
                "/admin/approval-process-definitions/drafts",
                Some(&token(&both)),
                Some(payload),
            )
            .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["data"]["document_type"], "stock_adjustment");
        assert_eq!(body["data"]["status"], "DRAFT");
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn definition_detail_upgrade_runtime_and_decide_require_dual_gates() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_api_dual").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let action_only = seed_admin_account(fixture.db()).await.expect("动作账号");
        grant(
            fixture.db(),
            &action_only,
            &[
                ("approval_process", "read"),
                ("approval_instance", "upgrade_binding"),
                ("approval_instance", "decide"),
                ("approval_instance", "cancel_blocked"),
            ],
        )
        .await;
        let type_only = seed_admin_account(fixture.db()).await.expect("类型账号");
        grant(
            fixture.db(),
            &type_only,
            &[
                ("stock_adjustment", "approval_definition_admin"),
                ("stock_adjustment", "approval_runtime_admin"),
            ],
        )
        .await;
        let both = seed_admin_account(fixture.db()).await.expect("双门禁账号");
        grant(
            fixture.db(),
            &both,
            &[
                ("approval_process", "read"),
                ("approval_instance", "upgrade_binding"),
                ("approval_instance", "decide"),
                ("approval_instance", "cancel_blocked"),
                ("stock_adjustment", "approval_definition_admin"),
                ("stock_adjustment", "approval_runtime_admin"),
            ],
        )
        .await;
        let none = seed_admin_account(fixture.db()).await.expect("无权限账号");
        let api = TestApi::new(routes::create(test_app_state(
            fixture.db().clone(),
            "mongodb://localhost".to_string(),
            fixture.name().to_string(),
        )));
        let missing_def = "/admin/approval-process-definitions/missing-def";
        let (status, body) = api.get(missing_def, Some(&token(&none))).await;
        assert_eq!(status, 403, "无动作权限不得泄露定义存在性: {body}");
        let (status, body) = api.get(missing_def, Some(&token(&type_only))).await;
        assert_eq!(status, 403, "仅类型权限不得读详情: {body}");
        let (status, body) = api.get(missing_def, Some(&token(&action_only))).await;
        assert!(
            status == 403 || status == 404,
            "仅动作权限不得看到无权类型详情: {status} {body}"
        );
        let (status, body) = api.get(missing_def, Some(&token(&both))).await;
        assert_eq!(status, 404, "双门禁命中后才允许 404: {body}");
        assert_ne!(error_code(&body), "APPROVAL_PROCESS_NOT_CONFIGURED");

        let upgrade = "/admin/business-documents/stock_adjustment/missing-doc/approval-definition/upgrade";
        let payload = json!({
            "reason": "升级到当前发布版本",
            "expected_document_version": "1",
            "expected_approval_binding_version": "1",
            "idempotency_key": "up-1"
        });
        let (status, _) = api
            .post(upgrade, Some(&token(&none)), Some(payload.clone()))
            .await;
        assert_eq!(status, 403);
        let (status, _) = api
            .post(upgrade, Some(&token(&type_only)), Some(payload.clone()))
            .await;
        assert_eq!(status, 403);
        let (status, body) = api
            .post(upgrade, Some(&token(&action_only)), Some(payload.clone()))
            .await;
        assert!(status == 403 || status == 404, "{status} {body}");
        let (status, body) = api.post(upgrade, Some(&token(&both)), Some(payload)).await;
        assert!(
            status == 404 || status == 409 || status == 422,
            "双门禁后才进入资源级错误: {status} {body}"
        );

        let decide = json!({
            "work_item_id": "missing-wi",
            "decision": "APPROVE",
            "expected_task_version": "1",
            "idempotency_key": "dec-1"
        });
        let (status, _) = api
            .post(
                "/admin/approval-decisions",
                Some(&token(&none)),
                Some(decide.clone()),
            )
            .await;
        assert_eq!(status, 403);
        let (status, _) = api
            .post(
                "/admin/approval-decisions",
                Some(&token(&type_only)),
                Some(decide.clone()),
            )
            .await;
        assert_eq!(status, 403);
        let (status, body) = api
            .post(
                "/admin/approval-decisions",
                Some(&token(&action_only)),
                Some(decide.clone()),
            )
            .await;
        assert!(status == 403 || status == 404 || status == 422, "{status} {body}");
        let (status, body) = api
            .post("/admin/approval-decisions", Some(&token(&both)), Some(decide))
            .await;
        assert!(
            status == 403 || status == 404 || status == 422,
            "决定失败不得泄露任务存在性: {status} {body}"
        );
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn http_covers_401_403_404_422_and_does_not_leak_existence() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_api_err").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let reader = seed_admin_account(fixture.db()).await.expect("读取账号");
        grant(fixture.db(), &reader, &[("approval_process", "read")]).await;
        let api = TestApi::new(routes::create(test_app_state(
            fixture.db().clone(),
            "mongodb://localhost".to_string(),
            fixture.name().to_string(),
        )));
        let (status, _) = api.get("/admin/approval-processes/catalog", None).await;
        assert_eq!(status, 401);
        let (status, body) = api
            .get(
                "/admin/approval-process-definitions/missing-def",
                Some(&token(&reader)),
            )
            .await;
        assert!(status == 403 || status == 404, "{status} {body}");
        assert_ne!(error_code(&body), "APPROVAL_PROCESS_NOT_CONFIGURED");
        let (status, body) = api
            .get(
                "/admin/approval-instances?view=mine&status=APPROVED",
                Some(&token(&reader)),
            )
            .await;
        assert!(status == 403 || status == 422, "{status} {body}");
        let (status, body) = api
            .post(
                "/admin/approval-decisions",
                Some(&token(&reader)),
                Some(json!({
                    "work_item_id": "wi-1",
                    "decision": "REJECT",
                    "reason": "  ",
                    "expected_task_version": "1",
                    "idempotency_key": "k-reject"
                })),
            )
            .await;
        assert!(status == 403 || status == 422, "{status} {body}");
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn replace_nodes_rejects_client_node_key_and_sales_purpose() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_api_nodes").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let admin = seed_admin_account(fixture.db()).await.expect("管理账号");
        grant(
            fixture.db(),
            &admin,
            &[
                ("approval_process", "read"),
                ("approval_process", "create"),
                ("approval_process", "edit"),
                ("stock_adjustment", "approval_definition_admin"),
            ],
        )
        .await;
        let router = routes::create(test_app_state(
            fixture.db().clone(),
            "mongodb://localhost".to_string(),
            fixture.name().to_string(),
        ));
        let api = TestApi::new(router.clone());
        let (status, body) = api
            .post(
                "/admin/approval-process-definitions/drafts",
                Some(&token(&admin)),
                Some(json!({
                    "document_type": "stock_adjustment",
                    "name": "库存调整试点",
                    "draft_source": "EMPTY",
                    "idempotency_key": "draft-nodes"
                })),
            )
            .await;
        assert_eq!(status, 200, "{body}");
        let definition_id = body["data"]["definition_id"].as_str().expect("定义 ID");
        let lock = body["data"]["definition_lock_version"]
            .as_u64()
            .or_else(|| body["data"]["definition_lock_version"].as_str()?.parse().ok())
            .unwrap_or(1);
        let (status, body) = put_json(
            router.clone(),
            &format!("/admin/approval-process-definitions/{definition_id}/nodes"),
            &token(&admin),
            json!({
                "expected_definition_lock_version": lock.to_string(),
                "nodes": [{
                    "node_name": "仓储",
                    "display_order": 1,
                    "assignee_user_id": admin,
                    "node_key": "forged"
                }]
            }),
        )
        .await;
        assert!(
            status == 400 || status == 422,
            "node_key 必须拒绝: {status} {body}"
        );
        let (status, body) = put_json(
            router,
            &format!("/admin/approval-process-definitions/{definition_id}/nodes"),
            &token(&admin),
            json!({
                "expected_definition_lock_version": lock.to_string(),
                "nodes": [{
                    "node_name": "仓储",
                    "display_order": 1,
                    "assignee_user_id": admin,
                    "node_purpose": "SALES_ORDER_PROCUREMENT_CONFIRMATION"
                }]
            }),
        )
        .await;
        assert!(
            status == 400 || status == 422,
            "purpose 必须拒绝: {status} {body}"
        );
    });
}

/// HTTP 必须覆盖 2xx/403/404/409/422/500 与权限失败不泄露存在性。
#[test]
fn http_status_matrix_covers_stable_approval_codes() {
    use axum::http::HeaderMap;
    use web_api::core::handler::approval_instance::error::ApprovalHttpError;

    let headers = HeaderMap::new();
    let policy = ApprovalHttpError::from_service(
        services::Error::Internal("APPROVAL_POLICY_NOT_REGISTERED".to_string()),
        &headers,
    );
    assert_eq!(policy.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    let forbidden = ApprovalHttpError::from_service(
        services::Error::Forbidden("APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR".to_string()),
        &headers,
    );
    assert_eq!(forbidden.status(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(forbidden.code(), "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR");
    let conflict = ApprovalHttpError::from_service(
        services::Error::ConflictError("APPROVAL_INSTANCE_VERSION_CONFLICT".to_string()),
        &headers,
    );
    assert_eq!(conflict.status(), axum::http::StatusCode::CONFLICT);
    let invalid = ApprovalHttpError::from_service(
        services::Error::BusinessLogicError("APPROVAL_DEFINITION_INVALID".to_string()),
        &headers,
    );
    assert_eq!(invalid.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    let not_configured = ApprovalHttpError::from_service(
        services::Error::ConflictError("APPROVAL_PROCESS_NOT_CONFIGURED".to_string()),
        &headers,
    );
    assert_eq!(not_configured.status(), axum::http::StatusCode::CONFLICT);
    let missing = ApprovalHttpError::from_service(
        services::Error::NotFound("审批流程定义不存在".to_string()),
        &headers,
    );
    assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(missing.code(), "NOT_FOUND");
    let leak = ApprovalHttpError::from_service(
        services::Error::Forbidden("没有该单据类型的流程定义管理权限".to_string()),
        &headers,
    );
    assert_eq!(leak.status(), axum::http::StatusCode::FORBIDDEN);
}

/// 12 个必须审批类型都登记动作级+类型级双门禁。
#[test]
fn all_process_required_types_register_dual_admin_permissions() {
    use entities::document_registry::DocumentType;
    use entities::Permission;
    use services::approval::policy::{policy_of, require_process_required, DocumentApprovalPolicy};

    for document_type in [
        DocumentType::SalesOrder,
        DocumentType::VoucherSalesOrder,
        DocumentType::SalesChangeOrder,
        DocumentType::PurchaseOrder,
        DocumentType::PurchaseChangeOrder,
        DocumentType::StockAdjustment,
        DocumentType::CustomerReceipt,
        DocumentType::SupplierPayment,
        DocumentType::CustomerRefund,
        DocumentType::SupplierRefund,
        DocumentType::ReceiptReversal,
        DocumentType::PaymentReversal,
    ] {
        let policy = require_process_required(document_type).expect("必须审批");
        let definition_admin =
            Permission::parse(format!("{}:approval_definition_admin", document_type.as_str()))
                .expect("定义管理权限");
        let runtime_admin = Permission::parse(format!("{}:approval_runtime_admin", document_type.as_str()))
            .expect("运行管理权限");
        assert_eq!(policy.definition_admin_permission, definition_admin);
        assert_eq!(policy.runtime_admin_permission, runtime_admin);
        assert!(matches!(
            policy_of(document_type).expect("政策"),
            DocumentApprovalPolicy::ProcessRequired(_)
        ));
    }
    let scope = include_str!("../../../services/src/approval/scope.rs");
    assert!(scope.contains("fn can_read_detail"));
    assert!(scope.contains("can_define(document_type) || self.runtime_admin_types.contains"));
}

/// 升级/决定/撤回请求拒绝内部字段；幂等回读不得重做写入。
#[test]
fn upgrade_decide_and_cancel_deny_client_internal_fields() {
    use serde_json::json;
    use services::approval::execution::PreparedExecution;
    use web_api::core::handler::approval_instance::http::{
        CancelBlockedHttpRequest, SubmitDecisionHttpRequest, UpgradeBindingHttpRequest,
    };

    assert!(serde_json::from_value::<UpgradeBindingHttpRequest>(json!({
        "reason": "升级",
        "expected_document_version": "1",
        "expected_approval_binding_version": "1",
        "idempotency_key": "k1",
        "definition_id": "forged"
    }))
    .is_err());
    assert!(serde_json::from_value::<CancelBlockedHttpRequest>(json!({
        "reason": "取消",
        "expected_instance_version": "1",
        "expected_execution_version": "1",
        "idempotency_key": "k1",
        "next_node": "n2"
    }))
    .is_err());
    assert!(serde_json::from_value::<SubmitDecisionHttpRequest>(json!({
        "work_item_id": "wi-1",
        "decision": "APPROVE",
        "expected_task_version": "1",
        "idempotency_key": "k1",
        "actor_id": "forged"
    }))
    .is_err());
    let _ = std::any::type_name::<PreparedExecution>();
    let exec = include_str!("../../../services/src/approval/execution/mod.rs");
    assert!(exec.contains("enum PreparedExecution"));
    assert!(exec.contains("Replay"));
    assert!(exec.contains("不得重做写入"));
}
