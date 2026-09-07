//! 阶段 17 HTTP 黄金矩阵：只构造内存错误，实际经过 From 与 IntoResponse。

use super::Error;
use axum::body::to_bytes;
use axum::http::header::{CONTENT_TYPE, RETRY_AFTER};
use axum::response::IntoResponse;
use erp_workflow::ErrorCode;
use mongodb::bson::{deserialize_from_document, doc};
use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};
use serde_json::{json, Value};

const INTERNAL_MESSAGE: &str = "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员";
const UNKNOWN_MESSAGE: &str = "操作结果暂无法确认，请先查询当前状态，确认未处理后再决定是否重试";
const CONFLICT_MESSAGE: &str = "当前资料状态不允许继续操作，请刷新后核对";
const BUSINESS_MESSAGE: &str = "当前业务条件不允许继续操作，请核对相关资料后重试";
const FORBIDDEN_MESSAGE: &str = "当前账号没有执行此操作的权限，请联系管理员或有权限的同事";
const DUPLICATE_MESSAGE: &str = "数据已存在，请勿重复提交";

struct Expected {
    status: u16,
    code: &'static str,
    message: &'static str,
    retryable: bool,
    field_errors: Option<Value>,
    retry_after: Option<&'static str>,
}

impl Expected {
    fn new(status: u16, code: &'static str, message: &'static str, retryable: bool) -> Self {
        Self {
            status,
            code,
            message,
            retryable,
            field_errors: None,
            retry_after: None,
        }
    }

    fn internal() -> Self {
        Self::new(500, "INTERNAL_ERROR", INTERNAL_MESSAGE, true)
    }

    fn conflict(message: &'static str) -> Self {
        Self::new(409, "CONFLICT", message, false)
    }
}

/// 核对实际 HTTP status、完整 JSON 信封和响应头，不复用被测分类/消息函数。
async fn assert_response(label: &str, error: Error, expected: Expected) {
    let response = error.into_response();
    assert_eq!(
        response.status().as_u16(),
        expected.status,
        "{label}: HTTP status"
    );
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json"),
        "{label}: Content-Type"
    );
    assert_eq!(
        response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
        expected.retry_after,
        "{label}: Retry-After"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("HTTP body");
    let actual: Value = serde_json::from_slice(&bytes).expect("HTTP JSON");
    let mut body = json!({
        "status": expected.status,
        "errorMessage": expected.message,
        "code": expected.code,
        "retryable": expected.retryable,
        "data": null,
        "success": false,
    });
    if let Some(fields) = expected.field_errors {
        body["fieldErrors"] = fields;
    }
    assert_eq!(actual, body, "{label}: complete response envelope");
}

fn duplicate(index: Option<&str>) -> persistence_core::Error {
    let message = match index {
        Some(name) => {
            format!("E11000 duplicate key error collection: erp.fixture index: {name} dup key: {{}}")
        }
        None => "E11000 duplicate key without an index name".to_string(),
    };
    let write: WriteError = deserialize_from_document(doc! {
        "code": 11000, "codeName": "DuplicateKey", "errmsg": message, "errInfo": null,
    })
    .expect("Mongo write error fixture");
    persistence_core::Error::from(MongoError::from(ErrorKind::Write(WriteFailure::WriteError(
        write,
    ))))
}

fn database_error() -> persistence_core::Error {
    persistence_core::Error::DatabaseError(MongoError::custom("database password leaked"))
}

fn unknown() -> persistence_core::Error {
    persistence_core::Error::CommitOutcomeUnknown(MongoError::custom("driver commit details"))
}

fn transient() -> persistence_core::Error {
    persistence_core::Error::TransientTransactionConflict(MongoError::custom("driver conflict details"))
}

/// 每个条目直接构造实际提供方的 variant，黄金预期与生产 Error 映射解耦。
macro_rules! common_cases {
    ($provider:ident) => {{
        use $provider::Error as Source;
        vec![
            (
                "Internal",
                Error::from(Source::Internal("database password leaked".into())),
                Expected::internal(),
            ),
            (
                "NotFound",
                Error::from(Source::NotFound("该订单不存在，请刷新后重试".into())),
                Expected::new(404, "NOT_FOUND", "该订单不存在，请刷新后重试", false),
            ),
            (
                "ValidationError",
                Error::from(Source::ValidationError("名称不能为空，请填写后重试".into())),
                Expected::new(400, "INVALID_REQUEST", "名称不能为空，请填写后重试", false),
            ),
            (
                "BusinessLogicError",
                Error::from(Source::BusinessLogicError(
                    "金额超过可用余额，请核对后重试".into(),
                )),
                Expected::new(
                    422,
                    "BUSINESS_RULE_BLOCKED",
                    "金额超过可用余额，请核对后重试",
                    false,
                ),
            ),
            (
                "ConflictError",
                Error::from(Source::ConflictError("资料已更新，请刷新后重试".into())),
                Expected::conflict("资料已更新，请刷新后重试"),
            ),
            (
                "ReceiptDuplicate",
                Error::from(Source::ReceiptDuplicate(duplicate(None))),
                Expected::conflict(DUPLICATE_MESSAGE),
            ),
            // 原瞬态冲突文案含技术术语“事务”，实际 HTTP 使用既有安全 fallback。
            (
                "TransientTransaction",
                Error::from(Source::TransientTransaction(transient())),
                Expected::conflict(CONFLICT_MESSAGE),
            ),
            (
                "Forbidden",
                Error::from(Source::Forbidden("当前账号不能查看资料，请联系管理员".into())),
                Expected::new(
                    403,
                    "PERMISSION_DENIED",
                    "当前账号不能查看资料，请联系管理员",
                    false,
                ),
            ),
            (
                "Unauthenticated",
                Error::from(Source::Unauthenticated("token secret leaked".into())),
                Expected::new(401, "UNAUTHENTICATED", "登录状态已失效，请重新登录", false),
            ),
            (
                "Logic",
                Error::from(Source::Logic(erp_core::Error::from("余额不足，请核对后重试"))),
                Expected::new(422, "BUSINESS_RULE_BLOCKED", "余额不足，请核对后重试", false),
            ),
            (
                "OutcomeUnknown",
                Error::from(Source::OutcomeUnknown(unknown())),
                Expected::new(500, "OUTCOME_UNKNOWN", UNKNOWN_MESSAGE, false),
            ),
            (
                "RepositoryError",
                Error::from(Source::RepositoryError(database_error())),
                Expected::internal(),
            ),
        ]
    }};
}

#[tokio::test]
async fn all_nineteen_domain_common_variants_keep_http_contract() {
    let domains = [
        ("identity", common_cases!(erp_identity)),
        ("audit", common_cases!(erp_audit)),
        ("workflow", common_cases!(erp_workflow)),
        ("support", common_cases!(erp_support)),
        ("party", common_cases!(erp_party)),
        ("customer", common_cases!(erp_customer)),
        ("supplier", common_cases!(erp_supplier)),
        ("catalog", common_cases!(erp_catalog)),
        ("warehouse", common_cases!(erp_warehouse)),
        ("contract", common_cases!(erp_contract)),
        ("inventory", common_cases!(erp_inventory)),
        ("finance", common_cases!(erp_finance)),
        ("sales", common_cases!(erp_sales)),
        ("procurement", common_cases!(erp_procurement)),
        ("fulfillment", common_cases!(erp_fulfillment)),
        ("returns", common_cases!(erp_returns)),
        ("integration", common_cases!(erp_integration)),
        ("supply", common_cases!(erp_supply)),
        ("import", common_cases!(erp_import)),
    ];
    for (domain, cases) in domains {
        assert_eq!(cases.len(), 12, "{domain}: common variant count");
        for (variant, error, expected) in cases {
            assert_response(&format!("{domain}::{variant}"), error, expected).await;
        }
    }
}

#[tokio::test]
async fn process_and_read_model_common_variants_keep_http_contract() {
    for (boundary, cases) in [
        ("process", common_cases!(erp_processes)),
        ("read_model", common_cases!(erp_read_models)),
    ] {
        for (variant, error, expected) in cases {
            assert_response(&format!("{boundary}::{variant}"), error, expected).await;
        }
    }
}

#[tokio::test]
async fn rbac_errors_remain_internal_at_every_real_provider() {
    for (label, error) in [
        (
            "identity",
            Error::from(erp_identity::Error::Rbac("secret policy details".into())),
        ),
        (
            "workflow",
            Error::from(erp_workflow::Error::Rbac("secret policy details".into())),
        ),
        (
            "process",
            Error::from(erp_processes::Error::Rbac("secret policy details".into())),
        ),
        (
            "read_model",
            Error::from(erp_read_models::Error::Rbac("secret policy details".into())),
        ),
    ] {
        assert_response(label, error, Expected::internal()).await;
    }
}

/// 审批冻结黄金值，不调用 ErrorCode::class/as_str/retryable 计算预期。
const APPROVAL_CODES: [(ErrorCode, &str, u16, bool); 21] = [
    (
        ErrorCode::ApprovalPolicyNotRegistered,
        "APPROVAL_POLICY_NOT_REGISTERED",
        500,
        false,
    ),
    (
        ErrorCode::ApprovalProcessNotConfigured,
        "APPROVAL_PROCESS_NOT_CONFIGURED",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalDraftSourceNotAvailable,
        "APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalDefinitionNotDraft,
        "APPROVAL_DEFINITION_NOT_DRAFT",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalDefinitionVersionConflict,
        "APPROVAL_DEFINITION_VERSION_CONFLICT",
        409,
        true,
    ),
    (
        ErrorCode::ApprovalDefinitionInvalid,
        "APPROVAL_DEFINITION_INVALID",
        422,
        false,
    ),
    (
        ErrorCode::ApprovalDefinitionBindingCorrupted,
        "APPROVAL_DEFINITION_BINDING_CORRUPTED",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalAlreadyStarted,
        "APPROVAL_ALREADY_STARTED",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalTaskNotOpen,
        "APPROVAL_TASK_NOT_OPEN",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalTaskNotAssignedToActor,
        "APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR",
        403,
        false,
    ),
    (
        ErrorCode::ApprovalTaskVersionConflict,
        "APPROVAL_TASK_VERSION_CONFLICT",
        409,
        true,
    ),
    (
        ErrorCode::ApprovalInstanceVersionConflict,
        "APPROVAL_INSTANCE_VERSION_CONFLICT",
        409,
        true,
    ),
    (
        ErrorCode::ApprovalExecutionVersionConflict,
        "APPROVAL_EXECUTION_VERSION_CONFLICT",
        409,
        true,
    ),
    (
        ErrorCode::ApprovalSubjectVersionConflict,
        "APPROVAL_SUBJECT_VERSION_CONFLICT",
        409,
        true,
    ),
    (
        ErrorCode::ApprovalRejectReasonRequired,
        "APPROVAL_REJECT_REASON_REQUIRED",
        422,
        false,
    ),
    (
        ErrorCode::ApprovalInstanceBlocked,
        "APPROVAL_INSTANCE_BLOCKED",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalResumeNotAllowedForBlocker,
        "APPROVAL_RESUME_NOT_ALLOWED_FOR_BLOCKER",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalCurrentApproverNotRecovered,
        "APPROVAL_CURRENT_APPROVER_NOT_RECOVERED",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalBlockedCancelNotAllowed,
        "APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalGenericWorkItemMutationForbidden,
        "APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN",
        409,
        false,
    ),
    (
        ErrorCode::ApprovalIdempotencyPayloadConflict,
        "APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT",
        409,
        false,
    ),
];

#[tokio::test]
async fn all_workflow_codes_keep_http_contract_across_three_boundaries() {
    for (code, text, status, retryable) in APPROVAL_CODES {
        let message = match status {
            500 => INTERNAL_MESSAGE,
            409 => CONFLICT_MESSAGE,
            422 => BUSINESS_MESSAGE,
            403 => FORBIDDEN_MESSAGE,
            _ => unreachable!("golden HTTP status"),
        };
        for (boundary, error) in [
            ("workflow", Error::from(erp_workflow::Error::Coded(code))),
            ("process", Error::from(erp_processes::Error::Coded(code))),
            ("read_model", Error::from(erp_read_models::Error::Coded(code))),
        ] {
            assert_response(
                &format!("{boundary}::{text}"),
                error,
                Expected::new(status, text, message, retryable),
            )
            .await;
        }
    }
}

const ORIGINAL_INDEXES: [(&str, &str); 17] = [
    ("uk_parties_party_no", "主体编号已存在，请核对当前资料后再操作"),
    (
        "uk_parties_credit_code",
        "统一社会信用代码已存在，请核对当前资料后再操作",
    ),
    (
        "uk_party_bank_accounts_bank_account_no",
        "银行账户编号已存在，请核对当前资料后再操作",
    ),
    (
        "uk_party_bank_accounts_party_hmac",
        "该主体下银行账号已存在，请核对当前资料后再操作",
    ),
    (
        "uk_supplier_accounts_party",
        "该主体已绑定供应商角色，请核对当前资料后再操作",
    ),
    (
        "uk_supplier_accounts_supplier_no",
        "供应商编号已存在，请核对当前资料后再操作",
    ),
    (
        "uk_supplier_offerings_supplier_sku",
        "该供应商 SKU 已登记供给，请核对当前资料后再操作",
    ),
    (
        "uk_contracts_contract_no",
        "合同编号已存在，请核对当前资料后再操作",
    ),
    (
        "uk_purchase_orders_creation_basis",
        "该采购创建依据已生成采购单，请核对当前资料后再操作",
    ),
    (
        "uk_customer_accounts_party",
        "该主体已绑定客户角色，请核对当前资料后再操作",
    ),
    (
        "uk_customer_accounts_customer_no",
        "客户编号已存在，请核对当前资料后再操作",
    ),
    (
        "uk_procurement_confirmation_lines_confirmation_line",
        "该采购确认已有相同分行序号，请刷新后重试",
    ),
    (
        "uk_procurement_confirmation_lines_active_confirmation_line",
        "该采购确认已有相同分行序号，请刷新后重试",
    ),
    (
        "uk_procurement_responsibility_active_selector",
        "同一采购责任选择器只能有一条启用规则，请核对当前资料后再操作",
    ),
    (
        "uk_work_items_open_fulfillment_object",
        "该履约对象已存在开放任务，请刷新后重试",
    ),
    (
        "uk_work_items_open_customer_acceptance_object",
        "该销售单已存在开放客户验收任务，请刷新后重试",
    ),
    (
        "uk_product_publication_revisions_publication_revision",
        "该发布修订序号已被占用，请刷新后重试",
    ),
];

const HISTORICAL_INDEXES: [(&str, &str); 3] =
    [ORIGINAL_INDEXES[11], ORIGINAL_INDEXES[12], ORIGINAL_INDEXES[16]];

#[tokio::test]
async fn duplicate_indexes_keep_seventeen_messages_and_exact_name_fallbacks() {
    let mut cases: Vec<(Option<String>, &str)> = ORIGINAL_INDEXES
        .iter()
        .map(|(index, message)| (Some((*index).to_string()), *message))
        .collect();
    cases.push((None, DUPLICATE_MESSAGE));
    cases.push((
        Some("uk_unregistered_business_key".to_string()),
        DUPLICATE_MESSAGE,
    ));
    for (index, _) in HISTORICAL_INDEXES {
        cases.push((Some(format!("prefix_{index}")), DUPLICATE_MESSAGE));
        cases.push((Some(format!("{index}_suffix")), DUPLICATE_MESSAGE));
    }
    assert_eq!(cases.len(), 25);
    for (index, message) in cases {
        let index = index.as_deref();
        for (boundary, error) in [
            ("direct_persistence", Error::from(duplicate(index))),
            (
                "process_from_persistence",
                Error::from(erp_processes::Error::from(duplicate(index))),
            ),
            (
                "read_model_from_persistence",
                Error::from(erp_read_models::Error::from(duplicate(index))),
            ),
        ] {
            assert_response(
                &format!("{boundary}:{index:?}"),
                error,
                Expected::conflict(message),
            )
            .await;
        }
    }
}

macro_rules! domain_historical_wrapper {
    ($provider:ident, $index:expr) => {
        (
            stringify!($provider),
            Error::from($provider::Error::RepositoryError(duplicate(Some($index)))),
        )
    };
}

#[tokio::test]
async fn historical_repository_wrappers_are_special_only_at_application_boundaries() {
    for (index, message) in HISTORICAL_INDEXES {
        for (boundary, error) in [
            (
                "process",
                Error::from(erp_processes::Error::RepositoryError(duplicate(Some(index)))),
            ),
            (
                "read_model",
                Error::from(erp_read_models::Error::RepositoryError(duplicate(Some(index)))),
            ),
        ] {
            assert_response(&format!("{boundary}:{index}"), error, Expected::conflict(message)).await;
        }
        let domain_wrappers = [
            domain_historical_wrapper!(erp_identity, index),
            domain_historical_wrapper!(erp_audit, index),
            domain_historical_wrapper!(erp_workflow, index),
            domain_historical_wrapper!(erp_support, index),
            domain_historical_wrapper!(erp_party, index),
            domain_historical_wrapper!(erp_customer, index),
            domain_historical_wrapper!(erp_supplier, index),
            domain_historical_wrapper!(erp_catalog, index),
            domain_historical_wrapper!(erp_warehouse, index),
            domain_historical_wrapper!(erp_contract, index),
            domain_historical_wrapper!(erp_inventory, index),
            domain_historical_wrapper!(erp_finance, index),
            domain_historical_wrapper!(erp_sales, index),
            domain_historical_wrapper!(erp_procurement, index),
            domain_historical_wrapper!(erp_fulfillment, index),
            domain_historical_wrapper!(erp_returns, index),
            domain_historical_wrapper!(erp_integration, index),
            domain_historical_wrapper!(erp_supply, index),
            domain_historical_wrapper!(erp_import, index),
        ];
        for (boundary, error) in domain_wrappers {
            assert_response(
                &format!("{boundary}::RepositoryError({index})"),
                error,
                Expected::internal(),
            )
            .await;
        }
    }
}

macro_rules! ordinary_repository_wrappers {
    ($provider:ident) => {{
        use $provider::Error as Source;
        [
            ("database", Error::from(Source::RepositoryError(database_error()))),
            (
                "optimistic",
                Error::from(Source::RepositoryError(
                    persistence_core::Error::OptimisticLockingError,
                )),
            ),
            ("unknown_commit", Error::from(Source::RepositoryError(unknown()))),
            ("transient", Error::from(Source::RepositoryError(transient()))),
            (
                "known_active_duplicate",
                Error::from(Source::RepositoryError(duplicate(Some("uk_parties_party_no")))),
            ),
            (
                "unknown_duplicate",
                Error::from(Source::RepositoryError(duplicate(Some(
                    "uk_unregistered_business_key",
                )))),
            ),
            (
                "unnamed_duplicate",
                Error::from(Source::RepositoryError(duplicate(None))),
            ),
        ]
    }};
}

#[tokio::test]
async fn other_application_repository_wrappers_remain_internal() {
    for (boundary, cases) in [
        ("process", ordinary_repository_wrappers!(erp_processes)),
        ("read_model", ordinary_repository_wrappers!(erp_read_models)),
    ] {
        for (kind, error) in cases {
            assert_response(
                &format!("{boundary}::RepositoryError({kind})"),
                error,
                Expected::internal(),
            )
            .await;
        }
    }
    for (historical, _) in HISTORICAL_INDEXES {
        for similar in [format!("prefix_{historical}"), format!("{historical}_suffix")] {
            for (boundary, error) in [
                (
                    "process",
                    Error::from(erp_processes::Error::RepositoryError(duplicate(Some(&similar)))),
                ),
                (
                    "read_model",
                    Error::from(erp_read_models::Error::RepositoryError(duplicate(Some(&similar)))),
                ),
            ] {
                assert_response(
                    &format!("{boundary}::RepositoryError({similar})"),
                    error,
                    Expected::internal(),
                )
                .await;
            }
        }
    }
}

#[tokio::test]
async fn direct_persistence_non_duplicate_errors_keep_http_contract() {
    for (label, error, expected) in [
        (
            "optimistic",
            Error::from(persistence_core::Error::OptimisticLockingError),
            Expected::conflict("数据已被其他请求修改，请刷新后重试"),
        ),
        (
            "transient",
            Error::from(transient()),
            Expected::conflict(CONFLICT_MESSAGE),
        ),
        (
            "unknown",
            Error::from(unknown()),
            Expected::new(500, "OUTCOME_UNKNOWN", UNKNOWN_MESSAGE, false),
        ),
        ("database", Error::from(database_error()), Expected::internal()),
    ] {
        assert_response(label, error, expected).await;
    }
}

fn validation_errors() -> validator::ValidationErrors {
    let mut errors = validator::ValidationErrors::new();
    let mut safe = validator::ValidationError::new("required");
    safe.message = Some("名称不能为空，请填写后重试".into());
    errors.add("name", safe);
    let mut unsafe_message = validator::ValidationError::new("length");
    unsafe_message.message = Some("Mongo password leaked".into());
    errors.add("secret", unsafe_message);
    errors
}

#[tokio::test]
async fn validator_fields_are_only_exposed_for_direct_http_validation() {
    let mut direct = Expected::new(
        400,
        "INVALID_REQUEST",
        "提交内容不符合要求，请根据字段提示修改后重试",
        false,
    );
    direct.field_errors = Some(json!({"name":"名称不能为空，请填写后重试","secret":"该字段填写不符合要求"}));
    assert_response("direct_validator", Error::from(validation_errors()), direct).await;
    for (label, error) in [
        (
            "process_validator",
            Error::from(erp_processes::Error::from(validation_errors())),
        ),
        (
            "read_model_validator",
            Error::from(erp_read_models::Error::from(validation_errors())),
        ),
    ] {
        assert_response(
            label,
            error,
            Expected::new(400, "INVALID_REQUEST", "提交内容不符合要求，请检查后重试", false),
        )
        .await;
    }
}

#[tokio::test]
async fn rate_limit_variants_keep_body_status_and_retry_after() {
    use crate::core::rate_limit::Error as RateError;
    for (label, source, status, message, retry_after) in [
        (
            "key",
            RateError::KeyExceeded { retry_after_secs: 12 },
            429,
            "请求过于频繁，请稍后重试",
            Some("12"),
        ),
        (
            "global",
            RateError::GlobalExceeded { retry_after_secs: 29 },
            429,
            "请求过于频繁，请稍后重试",
            Some("29"),
        ),
        (
            "concurrency",
            RateError::ConcurrencyExceeded,
            429,
            "请求过于频繁，请稍后重试",
            Some("1"),
        ),
        ("unavailable", RateError::Unavailable, 500, INTERNAL_MESSAGE, None),
    ] {
        let mut expected = Expected::new(status, "RATE_LIMITED", message, true);
        expected.retry_after = retry_after;
        assert_response(label, Error::from(source), expected).await;
    }
}
