//! 内部门户读取测试使用既有授权窄端口，不连接数据库。

use std::sync::{Arc, Mutex};

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::AccountKind;
use mongodb::Client;
use persistence_core::Executor;

use super::{PortalReadAuthorizationPort, SupplierPortalReadService};
use crate::{Error, Result};

#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Allowed,
    Denied,
    Failed,
}

pub(super) struct Authorization {
    outcome: Outcome,
    pub calls: Mutex<Vec<(&'static str, String, usize)>>,
}

impl Authorization {
    fn check(&self, kind: &'static str, id: &str, executor: &mut dyn Executor) -> Result<bool> {
        assert!(executor.boundary().unwrap().is_snapshot());
        self.calls.lock().unwrap().push((kind, id.into(), executor as *mut dyn Executor as *mut () as usize));
        match self.outcome {
            Outcome::Allowed => Ok(true),
            Outcome::Denied => Ok(false),
            Outcome::Failed => Err(Error::ConflictError("DATA_SCOPE_CHANGED：授权依赖已变化".into())),
        }
    }
}

#[async_trait]
impl PortalReadAuthorizationPort for Authorization {
    async fn request_readable(
        &self,
        _: &AuditActor,
        request: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.check("request", request, executor)
    }

    async fn supplier_readable(
        &self,
        _: &AuditActor,
        supplier: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.check("supplier", supplier, executor)
    }

    async fn offering_readable(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<bool> {
        panic!("内部门户列表与精确申请入口不得省略申请授权而直接检查供给")
    }
}

pub(super) async fn service(outcome: Outcome) -> (SupplierPortalReadService, Arc<Authorization>, AuditActor) {
    let client = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
    let authorization = Arc::new(Authorization { outcome, calls: Mutex::new(Vec::new()) });
    let service = SupplierPortalReadService::new(client.database("portal_read_authorization"))
        .with_authorization(authorization.clone());
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
    (service, authorization, actor)
}
