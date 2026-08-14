//! D03 阻塞审批诊断与恢复 HTTP 适配层。

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use entities::Permission;
use serde::Deserialize;
use services::{
    approval::{
        approval_management_scope, approval_recovery_authorization, approval_recovery_authorization_scope,
        approval_recovery_scope, BlockedApprovalListParams, BlockedApprovalPage, BlockedApprovalView,
        InternalApprovalRuntime, RecoverApprovalCommand,
    },
    audit::AuditActor,
    sales_review::CardSalesApprovalActionPort,
};

use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        response::ApiResponse,
    },
};

const RECOVER_PERMISSION: &str = "approval_instance:recover";

/// 阻塞审批恢复的 HTTP 请求封套。
///
/// 版本使用十进制字符串，避免浏览器数值精度改变服务端乐观锁事实。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverBlockedApprovalRequest {
    /// 查询所得当前阻塞步骤 ID。
    pub current_step_instance_id: String,
    /// 查询所得审批实例版本。
    pub expected_instance_version: String,
    /// 查询所得审批步骤版本。
    pub expected_step_version: String,
    /// 阻塞前存在开放待办时的查询版本。
    pub expected_task_version: Option<String>,
    /// 固定恢复动作。
    pub recovery_action: services::approval::ApprovalRecoveryAction,
    /// 结构化恢复原因。
    pub reason: String,
    /// 业务幂等键。
    pub idempotency_key: String,
}

#[permission_macros::permission(
    group = "阻塞审批管理",
    group_desc = "诊断并受控恢复无法安全路由的审批实例",
    desc = "按授权组织查询阻塞审批",
    resource = "approval_instance",
    action = "diagnose"
)]
/// 查询当前身份授权组织内的阻塞审批安全投影。
///
/// # 返回
/// 返回数据库侧组织过滤后的分页结果；无恢复权限时每条记录的
/// `allowed_actions` 为空。
pub async fn blocked_approval_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<BlockedApprovalListParams>,
) -> Result<BlockedApprovalPage> {
    let rbac = state.rbac();
    let scope = approval_management_scope(&state.db(), rbac.as_ref(), &actor).await?;
    let recover_permission =
        Permission::parse(RECOVER_PERMISSION).expect("固定审批恢复权限必须满足权限值对象合同");
    let can_recover = rbac
        .permissions(actor.kind(), actor.id())
        .await?
        .iter()
        .any(|permission| permission.covers(&recover_permission));
    let recovery_scope = if can_recover {
        Some(approval_recovery_scope(&state.db(), rbac.as_ref(), &actor).await?)
    } else {
        None
    };
    let runtime = management_runtime(&state);
    let page = runtime
        .blocked_approvals(&params, scope.organization_ids(), recovery_scope.as_ref())
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "阻塞审批管理",
    group_desc = "诊断并受控恢复无法安全路由的审批实例",
    desc = "重新解析并恢复原当前审批步骤",
    resource = "approval_instance",
    action = "recover"
)]
/// 仅以 `RETRY_CURRENT_STEP` 恢复路径中的原阻塞审批实例。
///
/// # 返回
/// 返回恢复后的当前运行时事实；版本、步骤或阻塞原因已变化时返回冲突。
pub async fn recover_blocked_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<RecoverBlockedApprovalRequest>,
) -> std::result::Result<Response, Error> {
    let mut command = request.into_command(id.clone(), actor.id())?;
    let rbac = state.rbac();
    let authorization = approval_recovery_authorization(&state.db(), rbac.as_ref(), &actor).await?;
    let scope = approval_recovery_authorization_scope(&authorization);
    command.authorization = Some(authorization);
    let management = management_runtime(&state);
    management
        .ensure_approval_in_management_scope(&command.approval_instance_id, &scope)
        .await?;
    let runtime = state.approval_runtime(Arc::new(CardSalesApprovalActionPort::new(state.db())));
    match runtime.recover_approval(command).await {
        Ok(view) => Ok(ApiResponse::ok_with_data(view).into_response()),
        Err(services::Error::ConflictError(message)) => {
            match management.blocked_approval(&id, &scope, true).await {
                Ok(latest) => Ok(recovery_conflict_response(message, latest)),
                Err(services::Error::ConflictError(_) | services::Error::NotFound(_)) => {
                    Err(Error::Conflict(message))
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

impl RecoverBlockedApprovalRequest {
    fn into_command(
        self,
        approval_instance_id: String,
        actor_id: &str,
    ) -> std::result::Result<RecoverApprovalCommand, Error> {
        Ok(RecoverApprovalCommand {
            approval_instance_id,
            current_step_instance_id: self.current_step_instance_id,
            expected_instance_version: parse_expected_version(&self.expected_instance_version, "审批实例")?,
            expected_step_version: parse_expected_version(&self.expected_step_version, "审批步骤")?,
            expected_task_version: self
                .expected_task_version
                .as_deref()
                .map(|value| parse_expected_version(value, "审批任务"))
                .transpose()?,
            recovery_action: self.recovery_action,
            reason: self.reason,
            idempotency_key: self.idempotency_key,
            actor_id: actor_id.to_string(),
            authorization: None,
        })
    }
}

fn parse_expected_version(value: &str, label: &str) -> std::result::Result<u64, Error> {
    let version = value
        .parse::<u64>()
        .map_err(|_| Error::BadRequest(format!("{label}期望版本必须是正整数字符串")))?;
    if version == 0 {
        return Err(Error::BadRequest(format!("{label}期望版本必须是正整数字符串")));
    }
    Ok(version)
}

fn management_runtime(state: &AppState) -> InternalApprovalRuntime {
    InternalApprovalRuntime::new(state.db(), Arc::new(CardSalesApprovalActionPort::new(state.db())))
}

/// 构造携带最新阻塞事实的稳定 409 响应。
fn recovery_conflict_response(message: String, latest: BlockedApprovalView) -> Response {
    ApiResponse {
        status: StatusCode::CONFLICT.as_u16(),
        message: Error::Conflict(message).to_string(),
        code: Some("CONFLICT".to_string()),
        data: Some(latest),
        success: false,
    }
    .into_response()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use services::approval::ApprovalRecoveryAction;

    use super::RecoverBlockedApprovalRequest;

    #[test]
    fn recovery_http_envelope_parses_string_versions_and_injects_actor() {
        let command = RecoverBlockedApprovalRequest {
            current_step_instance_id: "step-1".to_string(),
            expected_instance_version: "2".to_string(),
            expected_step_version: "3".to_string(),
            expected_task_version: Some("4".to_string()),
            recovery_action: ApprovalRecoveryAction::RetryCurrentStep,
            reason: "已补齐责任范围".to_string(),
            idempotency_key: "request-1".to_string(),
        }
        .into_command("instance-1".to_string(), "admin-1")
        .unwrap();

        assert_eq!(command.approval_instance_id, "instance-1");
        assert_eq!(command.actor_id, "admin-1");
        assert_eq!(command.expected_instance_version, 2);
        assert_eq!(command.expected_step_version, 3);
        assert_eq!(command.expected_task_version, Some(4));
    }

    #[test]
    fn recovery_http_envelope_rejects_non_positive_version_strings() {
        let request = RecoverBlockedApprovalRequest {
            current_step_instance_id: "step-1".to_string(),
            expected_instance_version: "0".to_string(),
            expected_step_version: "3".to_string(),
            expected_task_version: None,
            recovery_action: ApprovalRecoveryAction::RetryCurrentStep,
            reason: "已补齐责任范围".to_string(),
            idempotency_key: "request-1".to_string(),
        };

        assert!(request.into_command("instance-1".to_string(), "admin-1").is_err());
    }

    #[test]
    fn recovery_http_envelope_rejects_forbidden_fields() {
        let base = json!({
            "current_step_instance_id": "step-1",
            "expected_instance_version": "2",
            "expected_step_version": "3",
            "expected_task_version": "4",
            "recovery_action": "RETRY_CURRENT_STEP",
            "reason": "已补齐责任范围",
            "idempotency_key": "request-1",
            "target_user_id": "forged-user"
        });
        assert!(serde_json::from_value::<RecoverBlockedApprovalRequest>(base).is_err());

        let with_decision = json!({
            "current_step_instance_id": "step-1",
            "expected_instance_version": "2",
            "expected_step_version": "3",
            "recovery_action": "RETRY_CURRENT_STEP",
            "reason": "重试当前步骤",
            "idempotency_key": "request-2",
            "decision": "APPROVE"
        });
        assert!(serde_json::from_value::<RecoverBlockedApprovalRequest>(with_decision).is_err());
    }
}
