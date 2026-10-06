//! 内部专项接口仍使用真实后台身份、专属权限及目标对象范围。
use std::sync::Arc;

use application_core::{AuditActor, PageView};
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use erp_catalog::portal::{CatalogPortalService, DictionaryCandidate, DictionaryKind};
use erp_identity::{PortalAccountCreate, PortalAccountUpdate, PortalAccountView, PortalRole};
use erp_processes::adapters::purchase_access;
use erp_processes::adapters::workflow::workflow_auth;
use erp_processes::supplier_portal::authorization::PortalReadAuthorization;
use erp_processes::supplier_portal::{PortalGrantInput, PortalReview};
use erp_read_models::supplier_portal::{
    PortalAdminListParams, PortalListParams, PortalOfferingImpactReadService, SupplierPortalReadService,
};
use erp_supply::portal::QuoteAccessGrant;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};
use serde_json::{Value, to_value};

use super::process;
use super::read::DictionaryParams;
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::response::ApiResponse;

/// 协议层账号开通允许字段；内部角色及组织不属于外部身份。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountCreate {
    pub supplier_id: String,
    pub account: String,
    pub name: String,
    pub password: String,
    pub role: PortalRole,
    pub idempotency_key: String,
}

// HTTP 包装仅补充目标供应商和命令标识；账号业务字段由身份领域持有。
impl From<AccountCreate> for PortalAccountCreate {
    fn from(input: AccountCreate) -> Self {
        Self { account: input.account, name: input.name, password: input.password, role: input.role }
    }
}

/// 账号更新只允许固定岗位、启停及两个原版本。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountUpdate {
    pub expected_account_version: u64,
    pub expected_binding_version: u64,
    pub role: PortalRole,
    pub active: bool,
    pub idempotency_key: String,
}

// HTTP 命令标识不属于账号状态，转换保留领域的账号与绑定双版本。
impl From<AccountUpdate> for PortalAccountUpdate {
    fn from(input: AccountUpdate) -> Self {
        Self {
            expected_account_version: input.expected_account_version,
            expected_binding_version: input.expected_binding_version,
            role: input.role,
            active: input.active,
        }
    }
}

/// 查询目标供应商的具名门户账号。
/// # 参数
/// 内部身份、明确供应商及分页筛选。
/// # 返回
/// 不含凭证的账号页。
/// # 错误
/// 无供应商范围或查询非法时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "查询供应商门户账号",
    resource = "supplier_portal_account",
    action = "list"
)]
pub async fn accounts(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PortalAdminListParams>,
) -> Result<Value> {
    let data = reads(&state).accounts(&actor, &params).await?;
    ok_value(data)
}

/// 为已启用供应商开通具名门户账号。
/// # 参数
/// 独立账号资料、目标供应商与原操作号。
/// # 返回
/// 新账号的安全视图。
/// # 错误
/// 范围、身份、输入或账号唯一性不符时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "开通供应商门户账号",
    resource = "supplier_portal_account",
    action = "create"
)]
pub async fn create_account(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(input): Json<AccountCreate>,
) -> Result<PortalAccountView> {
    let supplier_id = input.supplier_id.clone();
    let idempotency_key = input.idempotency_key.clone();
    Ok(ApiResponse::ok_with_data(
        process(&state).account_create(&supplier_id, input.into(), &idempotency_key, &actor).await?,
    ))
}

/// 修改固定门户岗位或停用账号及绑定。
/// # 参数
/// 精确账号与原账号、绑定版本。
/// # 返回
/// 生效后的安全账号状态。
/// # 错误
/// 范围或版本失效时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "管理供应商门户账号",
    resource = "supplier_portal_account",
    action = "update"
)]
pub async fn update_account(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(input): Json<AccountUpdate>,
) -> Result<PortalAccountView> {
    let idempotency_key = input.idempotency_key.clone();
    Ok(ApiResponse::ok_with_data(
        process(&state).account_update(&id, input.into(), &idempotency_key, &actor).await?,
    ))
}

/// 查询明确供应商的定向SKU报价开放记录。
/// # 参数
/// 供应商对象范围内的分页筛选。
/// # 返回
/// SKU必要展示和开放版本。
/// # 错误
/// 无范围或读取失败时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "查询供应商定向报价目录",
    resource = "supplier_portal_catalog",
    action = "list"
)]
pub async fn grants(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PortalAdminListParams>,
) -> Result<Value> {
    ok_value(reads(&state).grants(&actor, &params).await?)
}

/// 开放或撤销精确供应商的精确SKU首次报价资格。
/// # 参数
/// 目标供应商、SKU与原开放版本。
/// # 返回
/// 当前开放事实与版本。
/// # 错误
/// 对象范围、目标状态或版本不符时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "维护供应商定向报价目录",
    resource = "supplier_portal_catalog",
    action = "update"
)]
pub async fn update_grant(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(input): Json<PortalGrantInput>,
) -> Result<QuoteAccessGrant> {
    Ok(ApiResponse::ok_with_data(process(&state).quote_access_update(input, &actor).await?))
}

/// 在申请对象范围内分页查询供应商专项申请。
/// # 参数
/// 目标供应商和内部身份。
/// # 返回
/// 仅有权读取的申请及对应总数。
/// # 错误
/// 范围、身份或分页不符时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "查询供应商门户申请",
    resource = "supplier_portal_request",
    action = "list"
)]
pub async fn applications(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<PortalAdminListParams>,
) -> Result<PageView<Value>> {
    Ok(ApiResponse::ok_with_data(reads(&state).admin_applications(&actor, &params).await?))
}

/// 读取可审核申请的冻结原稿、当前任务及明确复用目标。
/// # 参数
/// 精确申请及内部真实身份。
/// # 返回
/// 受对象资格保护的内部详情。
/// # 错误
/// 未知或越权统一404。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "查看供应商门户申请",
    resource = "supplier_portal_request",
    action = "detail"
)]
pub async fn application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Value> {
    let service = reads(&state);
    let mut data = service.admin_application(&actor, &id).await?;
    data["current"] = service.current_application_facts(&id, &actor, &mut NoTransaction).await?;
    if data.get("kind").and_then(Value::as_str) == Some("NEW_PRODUCT") {
        let candidates = service.existing_offering_review_candidates(&id, &actor, &mut NoTransaction).await?;
        data["existing_offerings"] =
            to_value(candidates).map_err(|error| Error::Internal(error.to_string()))?;
    }
    Ok(ApiResponse::ok_with_data(data))
}

/// 对当前申请及具体专项任务作确认或退回。
/// # 参数
/// 原申请、任务与匹配版本及明确决定。
/// # 返回
/// 实际正式结果或退回后的内部详情。
/// # 错误
/// 任一版本、范围、材料或处理资格失效时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "确认供应商门户申请",
    resource = "supplier_portal_request",
    action = "review"
)]
pub async fn review(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(input): Json<PortalReview>,
) -> Result<Value> {
    process(&state).review(&id, &input, &actor).await?;
    Ok(ApiResponse::ok_with_data(reads(&state).admin_application(&actor, &id).await?))
}

/// 查询审核可用的独立字典候选。
/// # 参数
/// 品牌、分类或单位及必要搜索。
/// # 返回
/// 有效字典最小投影，不授予字典维护权限。
/// # 错误
/// 类型或读取无效时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "查询供应商新品审核字典",
    resource = "supplier_portal_request",
    action = "review"
)]
pub async fn dictionaries(
    State(state): State<AppState>,
    Path(kind): Path<DictionaryKind>,
    Query(params): Query<DictionaryParams>,
) -> Result<Vec<DictionaryCandidate>> {
    let data = CatalogPortalService::new(state.db(), state.catalog_service())
        .dictionary_candidates(kind, params.q.as_deref(), params.product_kind, &mut NoTransaction)
        .await?;
    Ok(ApiResponse::ok_with_data(data))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateParams {
    pub q: Option<String>,
}

/// 查询精确新品申请的重复候选及已有供给版本。
/// # 参数
/// 受任务和对象授权保护的申请及搜索。
/// # 返回
/// 安全可读候选及精确冻结引用。
/// # 错误
/// 无申请或商品匹配资格时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "匹配供应商新品已有商品",
    resource = "supplier_portal_request",
    action = "review"
)]
pub async fn duplicates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<DuplicateParams>,
) -> Result<Value> {
    let catalog = CatalogPortalService::new(state.db(), state.catalog_service());
    let data = reads(&state)
        .new_product_review_context(&id, params.q.as_deref(), &actor, &catalog, &mut NoTransaction)
        .await?;
    ok_value(data)
}

/// 在真实供给及采购范围内查询未完成履约影响。
/// # 参数
/// 精确供给、内部身份与有界分页。
/// # 返回
/// 当前采购责任人及仍开放的真实履约任务，不返回财务字段。
/// # 错误
/// 来源、对象范围、当前任务资格或分页无效时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "核对供给未完成履约影响",
    resource = "supplier_portal_request",
    action = "detail"
)]
pub async fn impacts(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<PortalListParams>,
) -> Result<Value> {
    let service = PortalOfferingImpactReadService::new(
        state.db(),
        purchase_access(state.db(), state.rbac()),
        Arc::new(PortalReadAuthorization::new(state.db(), state.rbac())),
        workflow_auth(state.db(), state.rbac()),
    );
    ok_value(service.offering_impacts(&actor, &id, &params).await?)
}

fn reads(state: &AppState) -> SupplierPortalReadService {
    SupplierPortalReadService::new(state.db())
        .with_authorization(Arc::new(PortalReadAuthorization::new(state.db(), state.rbac())))
}

fn ok_value(data: impl Serialize) -> Result<Value> {
    Ok(ApiResponse::ok_with_data(to_value(data).map_err(|error| Error::Internal(error.to_string()))?))
}

#[cfg(test)]
mod tests {
    use serde_json::{from_value, json};

    use super::*;

    #[test]
    fn account_request_rejects_internal_roles_binding_override_and_missing_original_key() {
        let base = json!({"supplier_id":"supplier","account":"alice","name":"Alice","password":"secret123","role":"maintainer","idempotency_key":"original"});
        assert!(from_value::<AccountCreate>(base.clone()).is_ok());
        for field in ["role_id", "organization_id", "account_kind", "binding_version"] {
            let mut invalid = base.clone();
            invalid[field] = json!("override");
            assert!(from_value::<AccountCreate>(invalid).is_err());
        }
        let mut missing = base;
        missing.as_object_mut().unwrap().remove("idempotency_key");
        assert!(from_value::<AccountCreate>(missing).is_err());
    }
}
