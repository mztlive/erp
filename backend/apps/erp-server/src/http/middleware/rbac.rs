use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::{Context, Poll};

use application_core::AuditActor;
use async_trait::async_trait;
use axum::body::Body;
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use erp_identity::{AuthorizationService, Permission};
use persistence_core::NoTransaction;
use tower::{Layer, Service};
use tracing::{error, warn};

use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::permissions::PermissionBinding;

/// HTTP 层仅通过统一授权服务读取真实账号的功能资格。
#[async_trait]
trait HttpAuthorization: Send + Sync {
    async fn allows(&self, actor: &AuditActor, permission: &Permission) -> erp_identity::Result<bool>;
}

#[async_trait]
impl HttpAuthorization for AuthorizationService {
    async fn allows(&self, actor: &AuditActor, permission: &Permission) -> erp_identity::Result<bool> {
        self.has_permission(actor, permission, &mut NoTransaction).await
    }
}

/// 为路由附加 identity 统一功能资格检查。
///
/// # 参数
/// * `route` - 待保护路由
/// * `authorization` - 账号与功能资格统一入口
/// * `permission` - 所需权限
///
/// # 返回值
/// 返回带权限校验层的路由。
pub fn with_permission(
    route: MethodRouter<HttpState>,
    authorization: &AuthorizationService,
    permission: impl Into<PermissionBinding>,
) -> MethodRouter<HttpState> {
    route.route_layer(RbacAuthorizeLayer::new(
        std::sync::Arc::new(authorization.clone()),
        permission.into().required.permission(),
    ))
}

#[derive(Clone)]
struct RbacAuthorizeLayer {
    authorization: std::sync::Arc<dyn HttpAuthorization>,
    permission: Permission,
}

impl RbacAuthorizeLayer {
    fn new(authorization: std::sync::Arc<dyn HttpAuthorization>, permission: Permission) -> Self {
        Self { authorization, permission }
    }
}

impl<Inner> Layer<Inner> for RbacAuthorizeLayer {
    type Service = RbacAuthorizeService<Inner>;

    fn layer(&self, inner: Inner) -> Self::Service {
        RbacAuthorizeService {
            inner,
            authorization: self.authorization.clone(),
            permission: self.permission.clone(),
        }
    }
}

#[derive(Clone)]
struct RbacAuthorizeService<Inner> {
    inner: Inner,
    authorization: std::sync::Arc<dyn HttpAuthorization>,
    permission: Permission,
}

impl<Inner> Service<Request<Body>> for RbacAuthorizeService<Inner>
where
    Inner: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    Inner::Future: Send + 'static,
{
    type Response = Response;
    type Error = Inner::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let authorization = self.authorization.clone();
        let permission = self.permission.clone();
        let subject = request.extensions().get::<AuditActor>().cloned();

        Box::pin(async move {
            let Some(subject) = subject else {
                warn!(event_kind = "authorization_attempt", result = "rejected", permission = %permission, "Identity authorization denied request without authenticated actor");
                return Ok(ApiResponse::<()>::unauthorized().into_response());
            };

            match authorization.allows(&subject, &permission).await {
                Ok(true) => {
                    poll_fn(|context| inner.poll_ready(context)).await?;
                    inner.call(request).await
                },
                Ok(false) => {
                    warn!(
                        event_kind = "authorization_attempt",
                        result = "rejected",
                        subject = %subject.id(),
                        permission = %permission,
                        "Identity authorization denied request"
                    );
                    Ok(ApiResponse::<()>::missing_permission(&permission).into_response())
                },
                Err(err) => {
                    error!(
                        event_kind = "authorization_attempt",
                        result = "unknown",
                        subject = %subject.id(),
                        permission = %permission,
                        error = %err,
                        "Identity authorization failed"
                    );
                    Ok(ApiResponse::<()>::system_error().into_response())
                },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::Arc;

    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use tower::{ServiceExt, service_fn};

    use super::*;

    struct Authorization(Option<bool>);

    #[async_trait]
    impl HttpAuthorization for Authorization {
        async fn allows(&self, _: &AuditActor, _: &Permission) -> erp_identity::Result<bool> {
            self.0.ok_or_else(|| erp_identity::Error::Internal("private provider error".into()))
        }
    }

    #[tokio::test]
    async fn route_reports_only_the_actual_denied_permission() {
        for (allowed, authenticated, status, key) in [
            (Some(false), true, StatusCode::FORBIDDEN, Some("sales_order:list")),
            (Some(true), true, StatusCode::OK, None),
            (Some(false), false, StatusCode::UNAUTHORIZED, None),
            (None, true, StatusCode::INTERNAL_SERVER_ERROR, None),
        ] {
            let layer = RbacAuthorizeLayer::new(
                Arc::new(Authorization(allowed)),
                Permission::parse("sales_order:list").unwrap(),
            );
            let inner = service_fn(|_: Request<Body>| async {
                Ok::<_, Infallible>(ApiResponse::ok().into_response())
            });
            let mut request = Request::new(Body::empty());
            if authenticated {
                request.extensions_mut().insert(AuditActor::new(
                    "user".into(),
                    "user".into(),
                    erp_core::AccountKind::Admin,
                ));
            }
            let response = layer.layer(inner).oneshot(request).await.unwrap();
            assert_eq!(response.status(), status);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json.get("requiredPermission").and_then(|v| v.as_str()), key);
            assert!(!String::from_utf8(body.to_vec()).unwrap().contains("private provider error"));
        }
    }
}

#[cfg(test)]
#[path = "rbac_contract.rs"]
mod contract_tests;
