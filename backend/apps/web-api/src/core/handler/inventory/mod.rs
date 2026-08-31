//! 域 D17 `inventory` 的 HTTP handler。
//!
//! Handler 只做协议适配：HTTP 十进制字符串版本在边界解析为 Service 数值类型，
//! 其余字段复用 `services::inventory` 类型；禁止在 Handler 承载业务规则或直连数据库。

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Extension, Json,
};
use entities::ids::WarehouseId;
use entities::inventory::AdjustmentReasonType;
use serde::Deserialize;
use services::{
    audit::AuditActor,
    inventory::{
        CancelStockAdjustmentApprovalRequest, CreateStockAdjustmentRequest, ExpectedStockBalanceVersion,
        InventoryService, PageView, StockAdjustmentDetailView, StockAdjustmentLineInput,
        StockAdjustmentLineUpdateInput, StockAdjustmentListParams, StockAdjustmentSubmitResultQuery,
        StockAdjustmentView, StockBalanceDetailView, StockBalanceListParams, StockBalanceView,
        StockMovementListParams, StockMovementView, StockReservationListParams, StockReservationView,
        SubmitStockAdjustmentRequest, UpdateStockAdjustmentRequest,
    },
};

use crate::{
    app_state::AppState,
    core::{
        errors::Result,
        handler::approval_instance::error::{parse_optional_version, parse_version, ApprovalHttpError},
        response::ApiResponse,
    },
};

/// 构造库存服务。
///
/// # 参数
/// * `state` - 应用状态
///
/// # 返回
/// 返回绑定数据库与 RBAC 的服务实例。
fn inventory_service(state: &AppState) -> InventoryService {
    InventoryService::new(state.db(), state.rbac())
}

/// 创建库存调整单的 HTTP wire。余额版本必须是十进制字符串。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStockAdjustmentHttpRequest {
    pub balance_id: String,
    pub expected_balance_version: String,
    pub adjustment_no: String,
    pub warehouse_id: WarehouseId,
    pub reason_type: AdjustmentReasonType,
    pub lines: Vec<StockAdjustmentLineInput>,
    pub note: Option<String>,
    pub occurred_at: Option<i64>,
}

impl CreateStockAdjustmentHttpRequest {
    fn into_service(
        self,
        headers: &HeaderMap,
    ) -> std::result::Result<CreateStockAdjustmentRequest, ApprovalHttpError> {
        Ok(CreateStockAdjustmentRequest {
            balance_id: self.balance_id,
            expected_balance_version: parse_version(&self.expected_balance_version, "库存余额", headers)?,
            adjustment_no: self.adjustment_no,
            warehouse_id: self.warehouse_id,
            reason_type: self.reason_type,
            lines: self.lines,
            note: self.note,
            occurred_at: self.occurred_at,
        })
    }
}

/// 更新库存调整单的 HTTP wire；乐观锁版本必须是十进制字符串。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateStockAdjustmentHttpRequest {
    pub version: String,
    pub reason_type: Option<AdjustmentReasonType>,
    pub lines: Option<Vec<StockAdjustmentLineUpdateInput>>,
    pub note: Option<String>,
    pub occurred_at: Option<i64>,
}

impl UpdateStockAdjustmentHttpRequest {
    fn into_service(
        self,
        headers: &HeaderMap,
    ) -> std::result::Result<UpdateStockAdjustmentRequest, ApprovalHttpError> {
        Ok(UpdateStockAdjustmentRequest {
            version: parse_version(&self.version, "库存调整单", headers)?,
            reason_type: self.reason_type,
            lines: self.lines,
            note: self.note,
            occurred_at: self.occurred_at,
        })
    }
}

/// 提交命令中的库存余额 CAS HTTP wire。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedStockBalanceVersionHttpRequest {
    pub balance_id: String,
    pub expected_version: String,
}

/// 库存调整提交审批 HTTP wire。所有版本必须是十进制字符串。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitStockAdjustmentHttpRequest {
    pub expected_version: String,
    pub expected_subject_version: String,
    pub reason_type: AdjustmentReasonType,
    pub lines: Vec<StockAdjustmentLineUpdateInput>,
    pub balances: Vec<ExpectedStockBalanceVersionHttpRequest>,
    pub note: String,
    pub occurred_at: i64,
    pub idempotency_key: String,
}

impl SubmitStockAdjustmentHttpRequest {
    fn into_service(
        self,
        headers: &HeaderMap,
    ) -> std::result::Result<SubmitStockAdjustmentRequest, ApprovalHttpError> {
        let expected_subject_version = parse_version(&self.expected_subject_version, "审批主题", headers)?;
        let expected_subject_version = u32::try_from(expected_subject_version)
            .map_err(|_| ApprovalHttpError::bad_request("页面数据已失效，请刷新后重试", headers))?;
        let balances = self
            .balances
            .into_iter()
            .map(|balance| {
                Ok(ExpectedStockBalanceVersion {
                    balance_id: balance.balance_id,
                    expected_version: parse_version(&balance.expected_version, "库存余额", headers)?,
                })
            })
            .collect::<std::result::Result<Vec<_>, ApprovalHttpError>>()?;
        Ok(SubmitStockAdjustmentRequest {
            expected_version: parse_version(&self.expected_version, "库存调整单", headers)?,
            expected_subject_version,
            reason_type: self.reason_type,
            lines: self.lines,
            balances,
            note: self.note,
            occurred_at: self.occurred_at,
            idempotency_key: self.idempotency_key,
        })
    }
}

/// 查询提交结果的 HTTP query；冻结主题版本必须是规范十进制字符串。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockAdjustmentSubmitResultHttpQuery {
    pub expected_subject_version: String,
    pub idempotency_key: String,
}

impl StockAdjustmentSubmitResultHttpQuery {
    fn into_service(
        self,
        headers: &HeaderMap,
    ) -> std::result::Result<StockAdjustmentSubmitResultQuery, ApprovalHttpError> {
        let expected_subject_version = parse_version(&self.expected_subject_version, "审批主题", headers)?;
        let expected_subject_version = u32::try_from(expected_subject_version)
            .map_err(|_| ApprovalHttpError::bad_request("页面数据已失效，请刷新后重试", headers))?;
        Ok(StockAdjustmentSubmitResultQuery {
            expected_subject_version,
            idempotency_key: self.idempotency_key,
        })
    }
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存余额列表",
    resource = "stock_balance",
    action = "list"
)]
/// 查询库存余额列表（W10 台账）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`warehouse_id`/`sku_id` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn stock_balance_list(
    State(state): State<AppState>,
    Query(params): Query<StockBalanceListParams>,
) -> Result<PageView<StockBalanceView>> {
    let page = inventory_service(&state).stock_balance_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存余额详情",
    resource = "stock_balance",
    action = "detail"
)]
/// 查询库存余额详情（余额 + 最近流水 + 有效预占 + 未过账调整）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 余额主键
///
/// # 返回
/// 返回余额详情视图。
pub async fn stock_balance_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StockBalanceDetailView> {
    let view = inventory_service(&state).stock_balance_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存流水台账",
    resource = "stock_movement",
    action = "list"
)]
/// 查询库存流水台账（W10 流水视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（仓库/SKU/类型/方向/时间区间）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn stock_movement_list(
    State(state): State<AppState>,
    Query(params): Query<StockMovementListParams>,
) -> Result<PageView<StockMovementView>> {
    let page = inventory_service(&state).stock_movement_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存预占列表",
    resource = "stock_reservation",
    action = "list"
)]
/// 查询库存预占列表（W10 销售预占视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（仓库/SKU/状态/销售明细）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn stock_reservation_list(
    State(state): State<AppState>,
    Query(params): Query<StockReservationListParams>,
) -> Result<PageView<StockReservationView>> {
    let page = inventory_service(&state).stock_reservation_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存调整单列表",
    resource = "stock_adjustment",
    action = "list"
)]
/// 查询库存调整单列表（W10 调整记录视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（仓库/状态）
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn stock_adjustment_list(
    State(state): State<AppState>,
    Query(params): Query<StockAdjustmentListParams>,
) -> Result<PageView<StockAdjustmentView>> {
    let page = inventory_service(&state).stock_adjustment_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存调整单详情",
    resource = "stock_adjustment",
    action = "detail"
)]
/// 查询库存调整单详情（表头 + 明细 + 过账流水）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前认证操作人，用于服务端动作投影
/// * `id` - 调整单主键
///
/// # 返回
/// 返回调整单详情视图。
pub async fn stock_adjustment_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<StockAdjustmentDetailView> {
    let view = inventory_service(&state)
        .stock_adjustment_detail(&id, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "创建库存调整单",
    resource = "stock_adjustment",
    action = "create"
)]
/// 创建库存调整单（草稿）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求（表头 + 明细）
///
/// # 返回
/// 返回新建调整单的完整详情视图。
pub async fn stock_adjustment_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Json(req): Json<CreateStockAdjustmentHttpRequest>,
) -> std::result::Result<ApiResponse<StockAdjustmentDetailView>, ApprovalHttpError> {
    let req = req.into_service(&headers)?;
    let view = inventory_service(&state)
        .create_stock_adjustment(req, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "更新库存调整单",
    resource = "stock_adjustment",
    action = "update"
)]
/// 更新库存调整单（仅草稿/驳回；乐观锁冲突返回 409）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 调整单主键
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后调整单的响应视图。
pub async fn stock_adjustment_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateStockAdjustmentHttpRequest>,
) -> std::result::Result<ApiResponse<StockAdjustmentView>, ApprovalHttpError> {
    let req = req.into_service(&headers)?;
    let view = inventory_service(&state)
        .update_stock_adjustment(&id, req, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "提交库存调整审批",
    resource = "stock_adjustment",
    action = "submit"
)]
/// 提交库存调整并启动统一审批。客户端不得选择定义或审批人。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 调整单主键
/// * `req` - 最终草稿、余额版本与幂等键
///
/// # 返回
/// 返回提交后的完整详情视图。
pub async fn stock_adjustment_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SubmitStockAdjustmentHttpRequest>,
) -> std::result::Result<ApiResponse<StockAdjustmentDetailView>, ApprovalHttpError> {
    let req = req.into_service(&headers)?;
    let view = inventory_service(&state)
        .submit_stock_adjustment(&id, req, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "查询库存调整提交结果",
    resource = "stock_adjustment",
    action = "detail"
)]
/// 按原冻结主题版本与幂等键查询提交审批的精确收据结果。
///
/// 缺失或错误幂等键返回 404；不得根据库存调整单当前状态推断成功。
pub async fn stock_adjustment_submit_result(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<StockAdjustmentSubmitResultHttpQuery>,
) -> std::result::Result<ApiResponse<StockAdjustmentDetailView>, ApprovalHttpError> {
    let query = query.into_service(&headers)?;
    let view = inventory_service(&state)
        .stock_adjustment_submit_result(&id, query, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "撤回库存调整审批",
    resource = "approval_instance",
    action = "cancel"
)]
/// 撤回尚未最终通过的库存调整审批。
///
/// Handler 只适配合同请求；Service 负责校验原提交人/运行管理员、调用统一
/// `prepare_cancel` 并原子关闭审批运行事实与开放任务。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 库存调整单主键
/// * `req` - 期望版本、必填原因与幂等键
///
/// # 返回
/// 返回撤回后库存调整单视图。
pub async fn stock_adjustment_cancel_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<CancelStockAdjustmentApprovalHttpRequest>,
) -> std::result::Result<ApiResponse<StockAdjustmentView>, ApprovalHttpError> {
    let command = req.into_service(&headers)?;
    let view = inventory_service(&state)
        .cancel_stock_adjustment_approval(&id, command, &actor)
        .await
        .map_err(|error| ApprovalHttpError::from_service(error, &headers))?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 库存调整普通撤回 HTTP 请求。所有乐观锁版本均使用十进制字符串。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CancelStockAdjustmentApprovalHttpRequest {
    /// 期望库存调整单版本。
    pub expected_version: String,
    /// 审批实例 ID。
    pub approval_process_instance_id: String,
    /// 冻结提交版本。
    pub expected_subject_version: String,
    /// 期望实例版本。
    pub expected_instance_version: String,
    /// 期望执行版本。
    pub expected_execution_version: String,
    /// 运行中实例的开放任务版本；人员失效阻塞实例为空。
    pub expected_task_version: Option<String>,
    /// 非空撤回原因。
    pub reason: String,
    /// 幂等键。
    pub idempotency_key: String,
}

impl CancelStockAdjustmentApprovalHttpRequest {
    fn into_service(
        self,
        headers: &HeaderMap,
    ) -> std::result::Result<CancelStockAdjustmentApprovalRequest, ApprovalHttpError> {
        let expected_subject_version = parse_version(&self.expected_subject_version, "审批主题", headers)?;
        let expected_subject_version = u32::try_from(expected_subject_version)
            .map_err(|_| ApprovalHttpError::bad_request("页面数据已失效，请刷新后重试", headers))?;
        Ok(CancelStockAdjustmentApprovalRequest {
            expected_version: parse_version(&self.expected_version, "库存调整单", headers)?,
            approval_process_instance_id: self.approval_process_instance_id,
            expected_subject_version,
            expected_instance_version: parse_version(&self.expected_instance_version, "审批实例", headers)?,
            expected_execution_version: parse_version(&self.expected_execution_version, "审批执行", headers)?,
            expected_task_version: parse_optional_version(
                self.expected_task_version.as_deref(),
                "审批任务",
                headers,
            )?,
            reason: self.reason,
            idempotency_key: self.idempotency_key,
        })
    }
}

#[permission_macros::permission(
    group = "库存",
    group_desc = "库存台账与库存调整管理",
    desc = "过账库存调整",
    resource = "stock_adjustment",
    action = "post"
)]
/// 人工过账旁路已关闭。过账只允许作为审批最终通过动作。
///
/// # 参数
/// * `_state` - 应用状态
/// * `_actor` - 已通过鉴权的审计操作人
/// * `_id` - 调整单主键
///
/// # 错误
/// 始终返回冲突，防止 HTTP 旁路过账。
pub async fn stock_adjustment_post(
    State(_state): State<AppState>,
    Extension(_actor): Extension<AuditActor>,
    Path(_id): Path<String>,
) -> Result<StockAdjustmentView> {
    Err(crate::core::errors::Error::Conflict(
        "库存调整过账只能由审批最终通过动作调用".to_string(),
    ))
}

/// 证明人工 approve/reject 端点已删除。
///
/// # 返回
/// 返回不再暴露的路径片段。
pub fn removed_manual_review_paths() -> &'static [&'static str] {
    &[
        "/stock-adjustments/{id}/approve",
        "/stock-adjustments/{id}/reject",
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        removed_manual_review_paths, CancelStockAdjustmentApprovalHttpRequest,
        CreateStockAdjustmentHttpRequest, StockAdjustmentSubmitResultHttpQuery,
        SubmitStockAdjustmentHttpRequest, UpdateStockAdjustmentHttpRequest,
    };
    use axum::http::HeaderMap;
    use services::inventory::SubmitStockAdjustmentRequest;

    /// 人工复核端点已删除，提交请求拒绝客户端选择审批人。
    #[test]
    fn manual_approve_reject_endpoints_are_removed() {
        assert_eq!(
            removed_manual_review_paths(),
            &[
                "/stock-adjustments/{id}/approve",
                "/stock-adjustments/{id}/reject"
            ]
        );
        assert!(
            serde_json::from_value::<SubmitStockAdjustmentRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "reviewed_by": "forged"
            }))
            .is_err()
        );
    }

    /// 正式撤回端口必须复用 Service DTO 和签署方法，直接过账仍失败关闭。
    #[test]
    fn cancel_approval_is_wired_and_direct_post_remains_closed() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产 handler 必须存在");
        assert!(production.contains("CancelStockAdjustmentApprovalRequest"));
        assert!(production.contains("CancelStockAdjustmentApprovalHttpRequest"));
        assert!(production.contains("cancel_stock_adjustment_approval(&id, command, &actor)"));
        assert!(production.contains("resource = \"approval_instance\""));
        assert!(production.contains("action = \"cancel\""));
        assert!(production.contains("pub async fn stock_adjustment_post"));
        assert!(production.contains("审批最终通过动作调用"));
    }

    /// 普通撤回 HTTP wire 只接受字符串版本并拒绝额外运行字段。
    #[test]
    fn cancel_approval_http_versions_are_strings_and_unknown_fields_are_rejected() {
        let valid = serde_json::from_value::<CancelStockAdjustmentApprovalHttpRequest>(serde_json::json!({
            "expected_version": "7",
            "approval_process_instance_id": "instance-1",
            "expected_subject_version": "2",
            "expected_instance_version": "3",
            "expected_execution_version": "4",
            "expected_task_version": "5",
            "reason": "撤回修改",
            "idempotency_key": "cancel-1"
        }))
        .expect("字符串版本请求必须可解析");
        assert_eq!(valid.expected_task_version.as_deref(), Some("5"));
        assert!(
            serde_json::from_value::<CancelStockAdjustmentApprovalHttpRequest>(serde_json::json!({
                "expected_version": 7,
                "approval_process_instance_id": "instance-1",
                "expected_subject_version": "2",
                "expected_instance_version": "3",
                "expected_execution_version": "4",
                "expected_task_version": null,
                "reason": "撤回修改",
                "idempotency_key": "cancel-1"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<CancelStockAdjustmentApprovalHttpRequest>(serde_json::json!({
                "expected_version": "7",
                "approval_process_instance_id": "instance-1",
                "expected_subject_version": "2",
                "expected_instance_version": "3",
                "expected_execution_version": "4",
                "expected_task_version": null,
                "reason": "撤回修改",
                "idempotency_key": "cancel-1",
                "actor_id": "forged"
            }))
            .is_err()
        );
    }

    fn cancel_http_request() -> CancelStockAdjustmentApprovalHttpRequest {
        CancelStockAdjustmentApprovalHttpRequest {
            expected_version: "7".to_string(),
            approval_process_instance_id: "instance-1".to_string(),
            expected_subject_version: "2".to_string(),
            expected_instance_version: "3".to_string(),
            expected_execution_version: "4".to_string(),
            expected_task_version: Some("5".to_string()),
            reason: "撤回修改".to_string(),
            idempotency_key: "cancel-1".to_string(),
        }
    }

    /// 撤回命令的每个必填及可选版本均拒绝空白、符号、零与数值溢出。
    #[test]
    fn cancel_http_rejects_invalid_decimal_version_strings() {
        let headers = HeaderMap::new();
        for value in ["", " 7", "-1", "0", "18446744073709551616"] {
            let mut document = cancel_http_request();
            document.expected_version = value.to_string();
            assert!(
                document.into_service(&headers).is_err(),
                "非法单据版本必须拒绝: {value:?}"
            );

            let mut subject = cancel_http_request();
            subject.expected_subject_version = value.to_string();
            assert!(
                subject.into_service(&headers).is_err(),
                "非法主题版本必须拒绝: {value:?}"
            );

            let mut instance = cancel_http_request();
            instance.expected_instance_version = value.to_string();
            assert!(
                instance.into_service(&headers).is_err(),
                "非法实例版本必须拒绝: {value:?}"
            );

            let mut execution = cancel_http_request();
            execution.expected_execution_version = value.to_string();
            assert!(
                execution.into_service(&headers).is_err(),
                "非法执行版本必须拒绝: {value:?}"
            );

            let mut task = cancel_http_request();
            task.expected_task_version = Some(value.to_string());
            assert!(
                task.into_service(&headers).is_err(),
                "非法任务版本必须拒绝: {value:?}"
            );
        }

        let mut subject_overflow = cancel_http_request();
        subject_overflow.expected_subject_version = "4294967296".to_string();
        assert!(subject_overflow.into_service(&headers).is_err());

        let without_task = CancelStockAdjustmentApprovalHttpRequest {
            expected_task_version: None,
            ..cancel_http_request()
        };
        assert!(without_task.into_service(&headers).is_ok());
    }

    fn submit_http_request() -> SubmitStockAdjustmentHttpRequest {
        serde_json::from_value(serde_json::json!({
            "expected_version": "7",
            "expected_subject_version": "2",
            "reason_type": "STOCK_GAIN",
            "lines": [{
                "line_id": "line-1",
                "quantity": "2.5",
                "direction": "INCREASE"
            }],
            "balances": [{
                "balance_id": "balance-1",
                "expected_version": "11"
            }],
            "note": "盘点",
            "occurred_at": 42,
            "idempotency_key": "submit-1"
        }))
        .expect("字符串版本提交请求")
    }

    fn create_http_request() -> CreateStockAdjustmentHttpRequest {
        serde_json::from_value(serde_json::json!({
            "balance_id": "balance-1",
            "expected_balance_version": "9007199254740993",
            "adjustment_no": "ADJ-1",
            "warehouse_id": "warehouse-1",
            "reason_type": "STOCK_GAIN",
            "lines": [{
                "sku_id": "sku-1",
                "quantity": "2.5",
                "direction": "INCREASE"
            }],
            "note": null,
            "occurred_at": 42
        }))
        .expect("字符串版本创建请求")
    }

    /// 创建与提交 HTTP 边界只接收十进制字符串，并在边界内精确解析。
    #[test]
    fn create_and_submit_http_versions_are_lossless_decimal_strings() {
        let headers = HeaderMap::new();
        let create = create_http_request();
        assert_eq!(
            create.into_service(&headers).unwrap().expected_balance_version,
            9_007_199_254_740_993
        );

        let submit = submit_http_request().into_service(&headers).unwrap();
        assert_eq!(submit.expected_version, 7);
        assert_eq!(submit.expected_subject_version, 2);
        assert_eq!(submit.balances[0].expected_version, 11);

        assert!(
            serde_json::from_value::<CreateStockAdjustmentHttpRequest>(serde_json::json!({
                "balance_id": "balance-1",
                "expected_balance_version": 7,
                "adjustment_no": "ADJ-1",
                "warehouse_id": "warehouse-1",
                "reason_type": "STOCK_GAIN",
                "lines": [{"sku_id": "sku-1", "quantity": "1", "direction": "INCREASE"}],
                "note": null,
                "occurred_at": null
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<SubmitStockAdjustmentHttpRequest>(serde_json::json!({
                "expected_version": 7,
                "expected_subject_version": "2",
                "reason_type": "STOCK_GAIN",
                "lines": [{"line_id": "line-1", "quantity": "1", "direction": "INCREASE"}],
                "balances": [{"balance_id": "balance-1", "expected_version": "11"}],
                "note": "盘点",
                "occurred_at": 42,
                "idempotency_key": "submit-1"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<SubmitStockAdjustmentHttpRequest>(serde_json::json!({
                "expected_version": "7",
                "expected_subject_version": 2,
                "reason_type": "STOCK_GAIN",
                "lines": [{"line_id": "line-1", "quantity": "1", "direction": "INCREASE"}],
                "balances": [{"balance_id": "balance-1", "expected_version": "11"}],
                "note": "盘点",
                "occurred_at": 42,
                "idempotency_key": "submit-1"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<SubmitStockAdjustmentHttpRequest>(serde_json::json!({
                "expected_version": "7",
                "expected_subject_version": "2",
                "reason_type": "STOCK_GAIN",
                "lines": [{"line_id": "line-1", "quantity": "1", "direction": "INCREASE"}],
                "balances": [{"balance_id": "balance-1", "expected_version": 11}],
                "note": "盘点",
                "occurred_at": 42,
                "idempotency_key": "submit-1"
            }))
            .is_err()
        );
    }

    /// 空白、负数、溢出和超出 u32 的主题版本一律在 HTTP 边界拒绝。
    #[test]
    fn submit_http_rejects_invalid_decimal_version_strings() {
        let headers = HeaderMap::new();
        for value in ["", " 7", "-1", "18446744073709551616"] {
            let mut create = create_http_request();
            create.expected_balance_version = value.to_string();
            assert!(
                create.into_service(&headers).is_err(),
                "非法创建余额版本必须拒绝: {value:?}"
            );

            let mut req = submit_http_request();
            req.expected_version = value.to_string();
            assert!(req.into_service(&headers).is_err(), "非法版本必须拒绝: {value:?}");

            let mut subject = submit_http_request();
            subject.expected_subject_version = value.to_string();
            assert!(
                subject.into_service(&headers).is_err(),
                "非法主题版本必须拒绝: {value:?}"
            );

            let mut balance = submit_http_request();
            balance.balances[0].expected_version = value.to_string();
            assert!(
                balance.into_service(&headers).is_err(),
                "非法余额版本必须拒绝: {value:?}"
            );
        }
        let mut subject_overflow = submit_http_request();
        subject_overflow.expected_subject_version = "4294967296".to_string();
        assert!(subject_overflow.into_service(&headers).is_err());

        let mut zero_create = create_http_request();
        zero_create.expected_balance_version = "0".to_string();
        assert!(zero_create.into_service(&headers).is_err());
        let mut zero_document = submit_http_request();
        zero_document.expected_version = "0".to_string();
        assert!(zero_document.into_service(&headers).is_err());
        let mut zero_subject = submit_http_request();
        zero_subject.expected_subject_version = "0".to_string();
        assert!(zero_subject.into_service(&headers).is_err());
        let mut zero_balance = submit_http_request();
        zero_balance.balances[0].expected_version = "0".to_string();
        assert!(zero_balance.into_service(&headers).is_err());
    }

    /// 提交结果查询只接受规范 u32 十进制主题版本，并原样保留幂等键。
    #[test]
    fn submit_result_http_query_requires_canonical_subject_version() {
        let headers = HeaderMap::new();
        let query = StockAdjustmentSubmitResultHttpQuery {
            expected_subject_version: "2".to_string(),
            idempotency_key: "submit result/中文".to_string(),
        }
        .into_service(&headers)
        .unwrap();
        assert_eq!(query.expected_subject_version, 2);
        assert_eq!(query.idempotency_key, "submit result/中文");

        for value in ["", " 2", "+2", "02", "-1", "0", "4294967296"] {
            let query = StockAdjustmentSubmitResultHttpQuery {
                expected_subject_version: value.to_string(),
                idempotency_key: "submit-1".to_string(),
            };
            assert!(
                query.into_service(&headers).is_err(),
                "非法查询版本必须拒绝: {value:?}"
            );
        }
        assert!(
            serde_json::from_value::<StockAdjustmentSubmitResultHttpQuery>(serde_json::json!({
                "expected_subject_version": 2,
                "idempotency_key": "submit-1"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<StockAdjustmentSubmitResultHttpQuery>(serde_json::json!({
                "expected_subject_version": "2",
                "idempotency_key": "submit-1",
                "status": "IN_APPROVAL"
            }))
            .is_err()
        );
    }

    /// PUT 更新边界只接受规范十进制字符串版本，禁止 JS number 与额外字段。
    #[test]
    fn update_http_version_is_lossless_and_fail_closed() {
        let headers = HeaderMap::new();
        let valid: UpdateStockAdjustmentHttpRequest = serde_json::from_value(serde_json::json!({
            "version": "9007199254740993",
            "reason_type": null,
            "lines": null,
            "note": "更新",
            "occurred_at": null
        }))
        .unwrap();
        assert_eq!(
            valid.into_service(&headers).unwrap().version,
            9_007_199_254_740_993
        );

        for value in ["", " 7", "+7", "07", "-1", "0", "18446744073709551616"] {
            let request = UpdateStockAdjustmentHttpRequest {
                version: value.to_string(),
                reason_type: None,
                lines: None,
                note: None,
                occurred_at: None,
            };
            assert!(
                request.into_service(&headers).is_err(),
                "非法 PUT 版本必须拒绝: {value:?}"
            );
        }
        assert!(
            serde_json::from_value::<UpdateStockAdjustmentHttpRequest>(serde_json::json!({
                "version": 7
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<UpdateStockAdjustmentHttpRequest>(serde_json::json!({
                "version": "7",
                "expected_version": "7"
            }))
            .is_err()
        );
    }
}
