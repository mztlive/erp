use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use erp_core::AccountKind;
use mongodb::Client;
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{ExecutionBoundary, NoTransaction};

use super::*;
use crate::ports::{InventoryAction, InventoryAuthorization};

#[derive(Clone, Copy)]
enum Rejection {
    Forbidden,
    Missing,
    Unavailable,
}

struct Authorization {
    rejection: Rejection,
    boundary: ExecutionBoundary,
    calls: AtomicUsize,
}

#[async_trait]
impl AuthorizationPort for Authorization {
    async fn require_adjustment_update(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        assert_eq!((actor.id(), id), ("actor", "adjustment"));
        assert!(self.boundary.is_current(executor));
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.rejection {
            Rejection::Forbidden => Err(Error::Forbidden("denied".into())),
            Rejection::Missing => Err(Error::NotFound("missing-source".into())),
            Rejection::Unavailable => Err(Error::Internal("authorization-unavailable".into())),
        }
    }

    async fn require_adjustment(&self, _: &AuditActor, _: &str, _: &mut dyn Executor) -> Result<()> {
        panic!("修改命令不得替换为详情资格")
    }

    async fn authorize(
        &self,
        _: &AuditActor,
        _: InventoryAction,
        _: &mut dyn Executor,
    ) -> Result<InventoryAuthorization> {
        panic!("修改命令不得发起列表授权")
    }

    async fn require_warehouse(
        &self,
        _: &AuditActor,
        _: InventoryAction,
        _: &str,
        _: &mut dyn Executor,
    ) -> Result<()> {
        panic!("既有调整单必须按实际对象授权，不得替换为仓库目标")
    }

    async fn allowed_warehouses(
        &self,
        _: &AuditActor,
        _: InventoryAction,
        _: &[String],
        _: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        panic!("修改命令不得枚举授权仓库")
    }
}

#[tokio::test]
async fn updates_authorize_the_actual_adjustment_before_business_reads_or_version_errors() {
    let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
        .await
        .unwrap()
        .database("adjustment_update_authorization_unit");
    let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
    let id = StockAdjustmentId::new("adjustment");
    for rejection in [Rejection::Forbidden, Rejection::Missing, Rejection::Unavailable] {
        let mut executor = SnapshotExecutor::default();
        let authorization =
            Authorization { rejection, boundary: executor.boundary().unwrap(), calls: AtomicUsize::new(0) };

        let result = adjustment_for_update(&db, &authorization, &actor, &id, 1, &mut NoTransaction).await;
        assert!(matches!(result, Err(Error::ValidationError(message))
            if message == "库存调整修改必须使用快照事务"));
        assert_eq!(authorization.calls.load(Ordering::SeqCst), 0);

        for version in [1, 999] {
            let result =
                adjustment_for_update(&db, &authorization, &actor, &id, version, &mut executor).await;
            match rejection {
                Rejection::Forbidden | Rejection::Missing => {
                    assert!(matches!(result, Err(Error::NotFound(message)) if message == "库存调整单不存在"));
                },
                Rejection::Unavailable => {
                    assert!(
                        matches!(result, Err(Error::Internal(message)) if message == "authorization-unavailable")
                    );
                },
            }
        }
        assert_eq!(authorization.calls.load(Ordering::SeqCst), 2);
    }
}
