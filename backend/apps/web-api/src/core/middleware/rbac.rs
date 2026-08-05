use std::{
    future::{poll_fn, Future},
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    body::Body,
    extract::Request,
    response::{IntoResponse, Response},
    routing::MethodRouter,
};
use entities::Permission;
use services::iam::SharedRbacService;
use tower::{Layer, Service};
use tracing::{error, warn};

use crate::{
    app_state::AppState,
    core::{middleware::RbacSubject, response::ApiResponse},
};

/// 为路由附加统一的 Casbin RBAC 权限校验。
///
/// # 返回值
/// 返回带权限校验层的路由。
pub fn with_permission(
    route: MethodRouter<AppState>,
    rbac_service: &SharedRbacService,
    permission: Permission,
) -> MethodRouter<AppState> {
    route.route_layer(RbacAuthorizeLayer::new(rbac_service.clone(), permission))
}

#[derive(Clone)]
struct RbacAuthorizeLayer {
    rbac_service: SharedRbacService,
    permission: Permission,
}

impl RbacAuthorizeLayer {
    fn new(rbac_service: SharedRbacService, permission: Permission) -> Self {
        Self {
            rbac_service,
            permission,
        }
    }
}

impl<Inner> Layer<Inner> for RbacAuthorizeLayer {
    type Service = RbacAuthorizeService<Inner>;

    fn layer(&self, inner: Inner) -> Self::Service {
        RbacAuthorizeService {
            inner,
            rbac_service: self.rbac_service.clone(),
            permission: self.permission.clone(),
        }
    }
}

#[derive(Clone)]
struct RbacAuthorizeService<Inner> {
    inner: Inner,
    rbac_service: SharedRbacService,
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
        let rbac_service = self.rbac_service.clone();
        let permission = self.permission.clone();
        let subject = request.extensions().get::<RbacSubject>().cloned();

        Box::pin(async move {
            let Some(subject) = subject else {
                warn!(permission = %permission, "RBAC denied request without authenticated subject");
                return Ok(ApiResponse::<()>::unauthorized().into_response());
            };

            match rbac_service.enforce(&subject.0, &permission).await {
                Ok(true) => {
                    poll_fn(|context| inner.poll_ready(context)).await?;
                    inner.call(request).await
                }
                Ok(false) => {
                    warn!(
                        subject = %subject.0,
                        permission = %permission,
                        "RBAC denied request"
                    );
                    Ok(ApiResponse::<()>::permission_denied().into_response())
                }
                Err(err) => {
                    error!(
                        subject = %subject.0,
                        permission = %permission,
                        error = %err,
                        "Casbin authorization failed"
                    );
                    Ok(ApiResponse::<()>::system_error().into_response())
                }
            }
        })
    }
}
