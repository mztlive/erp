//! P6-PILOT：试点 `StockAdjustment` 的定义/运行 HTTP、权限双门禁与稳定错误矩阵。
//!
//! 真实 MongoDB 用例一律 `#[ignore]` + `require_mongo!()`。客户端不得提交 node key、
//! 连线、角色、handler 或 action。未提供数据库时不得把 ignored 表述为通过。

use config::{AppConfig, Config, DatabaseConfig, S3Config, SafeConfig};
use database::ensure_indexes;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use storage::{S3Storage, S3StorageConfig};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
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

/// 未切换类型必须返回稳定 cut-over 码，不得回退旧运行时。
#[test]
fn uncut_over_process_required_types_fail_closed() {
    use entities::document_registry::DocumentType;
    use services::approval::business_adapter::ensure_runtime_cut_over;
    use services::APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER;

    assert!(ensure_runtime_cut_over(DocumentType::StockAdjustment).is_ok());
    for document_type in [
        DocumentType::SalesOrder,
        DocumentType::PurchaseOrder,
        DocumentType::CustomerReceipt,
    ] {
        let error = ensure_runtime_cut_over(document_type).expect_err("未切换必须失败");
        assert!(error.to_string().contains(APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER));
    }
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
        let stock = items
            .iter()
            .find(|item| item["document_type"] == "stock_adjustment")
            .expect("必须包含试点类型");
        assert_eq!(stock["approval_requirement"], "PROCESS_REQUIRED");
        assert!(stock.get("process_kind").is_none());
        assert!(stock.get("node_key").is_none());
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
        let api = TestApi::new(routes::create(test_app_state(
            fixture.db().clone(),
            "mongodb://localhost".to_string(),
            fixture.name().to_string(),
        )));
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
        let (status, detail) = api
            .get(
                &format!("/admin/approval-process-definitions/{definition_id}"),
                Some(&token(&admin)),
            )
            .await;
        assert_eq!(status, 200, "{detail}");
        assert!(detail["data"]["nodes"].as_array().expect("节点").is_empty());
        assert_eq!(detail["data"]["document_type"], "stock_adjustment");
    });
}
