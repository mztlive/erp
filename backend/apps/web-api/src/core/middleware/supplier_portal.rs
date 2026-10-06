//! 门户逐请求身份验证；不会向请求装入内部 Casbin 主体或组织身份。
use std::result::Result as StdResult;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use erp_core::AccountKind;
use erp_identity::{PortalActor, PortalIdentityService};
use erp_processes::Error as ProcessError;
use erp_processes::supplier_portal::SupplierPortalProcess;
use persistence_core::NoTransaction;

use super::authentication::bearer_token;
use crate::app_state::AppState;
use crate::core::auth::jwt::{SubjectKind, TokenPayload};
use crate::core::errors::Error;

/// 在每次访问时重读账号、绑定及供应商状态。
/// # 参数
/// `state` 是组合根，凭证来自标准 Authorization 请求头。
/// # 返回
/// 成功仅装入门户身份，继续门户处理器。
/// # 错误
/// 凭证类型、版本或当前状态无效时返回401；读取失败返回系统错误。
pub async fn authenticate_portal(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = bearer_token(request.headers()).map(str::to_owned);
    let result = match token {
        Some(token) => verified_actor(&state, &token).await,
        None => Err(invalid_session()),
    };
    match result {
        Ok(actor) => {
            request.extensions_mut().insert(actor);
            next.run(request).await
        },
        Err(error) => error.into_response(),
    }
}

async fn verified_actor(state: &AppState, token: &str) -> StdResult<PortalActor, Error> {
    let engine = state.jwt_engine().await.map_err(|error| Error::Internal(error.to_string()))?;
    let payload = engine.verify_token(token).map_err(|_| invalid_session())?;
    ensure_portal_payload(&payload)?;
    let actor = PortalIdentityService::new(state.db())
        .validate_session(
            &payload.id,
            &payload.account,
            payload.account_version.ok_or_else(invalid_session)?,
            payload.binding_version.ok_or_else(invalid_session)?,
            &mut NoTransaction,
        )
        .await?;
    match SupplierPortalProcess::new(state.db(), state.rbac())
        .session_validate(&actor, &mut NoTransaction)
        .await
    {
        Ok(actor) => Ok(actor),
        Err(ProcessError::Forbidden(_) | ProcessError::NotFound(_)) => Err(invalid_session()),
        Err(error) => Err(error.into()),
    }
}

fn invalid_session() -> Error {
    Error::Unauthorized("登录已失效，请重新登录".into())
}

fn ensure_portal_payload(payload: &TokenPayload) -> StdResult<(), Error> {
    if payload.subject_kind != SubjectKind::SupplierPortal
        || payload.account_kind != Some(AccountKind::Supplier)
        || payload.account_version.is_none()
        || payload.binding_version.is_none()
    {
        return Err(invalid_session());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_gate_accepts_only_supplier_subject_with_both_versions() {
        let mut payload = TokenPayload::supplier_portal("account-a".into(), "supplier01".into(), 1, 2);
        assert!(ensure_portal_payload(&payload).is_ok());
        payload.binding_version = None;
        assert!(ensure_portal_payload(&payload).is_err());
        assert!(
            ensure_portal_payload(&TokenPayload::backoffice(
                "a".into(),
                "admin".into(),
                AccountKind::Admin,
                1
            ))
            .is_err()
        );
    }
}
