#![cfg(test)]
//! 执行真实权限层，冻结全部既有权限的请求判定与响应合同。
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use application_core::AuditActor;
use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::response::IntoResponse;
use erp_identity::Permission;
use serde_json::{Value, json};
use tower::{Layer, ServiceExt, service_fn};

use super::{ApiResponse, HttpAuthorization, RbacAuthorizeLayer};
#[path = "rbac_contract_cases.rs"]
mod fixtures;

struct Authorization {
    result: Option<bool>,
    calls: Arc<Mutex<Vec<(String, String)>>>,
}

#[async_trait]
impl HttpAuthorization for Authorization {
    async fn allows(&self, subject: &AuditActor, permission: &Permission) -> erp_identity::Result<bool> {
        self.calls.lock().unwrap().push((subject.id().into(), permission.to_string()));
        self.result.ok_or_else(|| erp_identity::Error::Internal("private provider error".into()))
    }
}

#[tokio::test]
async fn existing_permissions_preserve_http_authorization_contract() {
    for (code, permission) in fixtures::cases() {
        assert_eq!(permission.to_string(), code);
        for (result, authenticated, status) in
            [(Some(true), true, 200), (Some(false), true, 403), (Some(true), false, 401), (None, true, 500)]
        {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let executed = Arc::new(AtomicUsize::new(0));
            let layer = RbacAuthorizeLayer::new(
                Arc::new(Authorization { result, calls: calls.clone() }),
                permission.clone(),
            );
            let count = executed.clone();
            let handler = service_fn(move |_: Request<Body>| {
                count.fetch_add(1, Ordering::SeqCst);
                async { Ok::<_, Infallible>(ApiResponse::ok_with_data("executed").into_response()) }
            });
            let mut request = Request::new(Body::empty());
            if authenticated {
                request.extensions_mut().insert(AuditActor::new(
                    "baseline-user".into(),
                    "user".into(),
                    erp_core::AccountKind::Admin,
                ));
            }
            let response = layer.layer(handler).oneshot(request).await.unwrap();
            assert_eq!(response.status().as_u16(), status, "{code}");
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body, expected_body(status, code), "{code}");
            assert_eq!(executed.load(Ordering::SeqCst), usize::from(status == 200));
            let expected = if authenticated { vec![("baseline-user".into(), code.into())] } else { vec![] };
            assert_eq!(*calls.lock().unwrap(), expected);
        }
    }
}

#[tokio::test]
async fn layered_permissions_keep_outer_first_denial_and_stop_execution() {
    use crate::permissions::{contract, customer};
    for (outer_result, inner_result, authenticated, status, denied) in [
        (Some(false), Some(true), true, 403, "customer_assignment:create"),
        (Some(true), Some(false), true, 403, "contract:update"),
        (Some(true), Some(true), true, 200, ""),
        (None, Some(true), true, 500, ""),
        (Some(true), None, true, 500, ""),
        (Some(true), Some(true), false, 401, ""),
    ] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let executed = Arc::new(AtomicUsize::new(0));
        let outer = RbacAuthorizeLayer::new(
            Arc::new(Authorization { result: outer_result, calls: calls.clone() }),
            customer::ASSIGNMENT_APPLY.permission(),
        );
        let inner = RbacAuthorizeLayer::new(
            Arc::new(Authorization { result: inner_result, calls: calls.clone() }),
            contract::SALES_ACCESS.permission(),
        );
        let count = executed.clone();
        let handler = service_fn(move |_: Request<Body>| {
            count.fetch_add(1, Ordering::SeqCst);
            async { Ok::<_, Infallible>(ApiResponse::ok_with_data("executed").into_response()) }
        });
        let mut request = Request::new(Body::empty());
        if authenticated {
            request.extensions_mut().insert(AuditActor::new(
                "baseline-user".into(),
                "user".into(),
                erp_core::AccountKind::Admin,
            ));
        }
        let response = outer.layer(inner.layer(handler)).oneshot(request).await.unwrap();
        assert_eq!(response.status().as_u16(), status);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), expected_body(status, denied));
        assert_eq!(executed.load(Ordering::SeqCst), usize::from(status == 200));
        let mut expected = Vec::new();
        if authenticated {
            expected.push(("baseline-user".into(), "customer_assignment:create".into()));
            if outer_result == Some(true) {
                expected.push(("baseline-user".into(), "contract:update".into()));
            }
        }
        assert_eq!(*calls.lock().unwrap(), expected);
    }
}

fn expected_body(status: u16, code: &str) -> Value {
    match status {
        200 => json!({"status":200,"errorMessage":"OK","data":"executed","success":true}),
        401 => {
            json!({"status":401,"errorMessage":"登录状态已失效，请重新登录","code":"UNAUTHENTICATED","retryable":false,"data":null,"success":false})
        },
        403 => {
            json!({"status":403,"errorMessage":format!("缺少功能权限：{code}，请联系管理员授权后重试"),"code":"PERMISSION_DENIED","requiredPermission":code,"retryable":false,"data":null,"success":false})
        },
        500 => {
            json!({"status":500,"errorMessage":"系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员","code":"INTERNAL_ERROR","retryable":true,"data":null,"success":false})
        },
        _ => unreachable!(),
    }
}
