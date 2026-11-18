//! 创建权限、销售来源及凭证资格全部通过后，才消费独立提交的验收编号。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::{FileAssetId, SalesOrderId};
use erp_fulfillment::service::document_number::next_customer_acceptance_no;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{fulfillment, sales_order};
use erp_read_models::sales_center::access::SalesAccess;
use mongodb::Database;
use persistence_core::Executor;

use super::evidence::ensure_evidence;
use crate::Result;
use crate::workflow::FulfillmentWorkflow;

#[async_trait]
trait NumberSource: Sync {
    async fn operation(&self, executor: &mut dyn Executor) -> Result<()>;
    async fn sales(&self, executor: &mut dyn Executor) -> Result<()>;
    async fn evidence(&self, executor: &mut dyn Executor) -> Result<()>;
    async fn next(&self) -> Result<String>;
}

async fn authorized_number(source: &impl NumberSource, executor: &mut dyn Executor) -> Result<String> {
    source.operation(executor).await?;
    source.sales(executor).await?;
    source.evidence(executor).await?;
    source.next().await
}

pub(super) struct CreationNumber<'a> {
    pub db: &'a Database,
    pub authorization: &'a Arc<AuthorizationService>,
    pub workflow: &'a dyn FulfillmentWorkflow,
    pub sales_order_id: &'a SalesOrderId,
    pub actor: &'a AuditActor,
    pub evidence: Option<&'a FileAssetId>,
}

impl CreationNumber<'_> {
    // 在调用方事务完成全部创建授权，再按既有永不回收合同独立取号。
    //
    // executor 为原创建事务执行器。返回已独立消费的客户验收编号。创建、销售来源、凭证资格拒绝或计数器分配失败时返回错误。
    pub(super) async fn allocate(&self, executor: &mut dyn Executor) -> Result<String> {
        authorized_number(self, executor).await
    }
}

#[async_trait]
impl NumberSource for CreationNumber<'_> {
    async fn operation(&self, executor: &mut dyn Executor) -> Result<()> {
        self.authorization
            .require(
                self.actor,
                &fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
                AccessTarget::Operation,
                executor,
            )
            .await?;
        Ok(())
    }

    async fn sales(&self, executor: &mut dyn Executor) -> Result<()> {
        SalesAccess::new(self.db.clone(), self.authorization.clone())
            .require_object(self.actor, &sales_order::DETAIL, self.sales_order_id.as_ref(), executor)
            .await?;
        Ok(())
    }

    async fn evidence(&self, executor: &mut dyn Executor) -> Result<()> {
        if let Some(asset_id) = self.evidence {
            ensure_evidence(self.db, self.workflow, asset_id, self.actor, executor).await?;
        }
        Ok(())
    }

    async fn next(&self) -> Result<String> {
        Ok(next_customer_acceptance_no(self.db).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    struct Source {
        reject: Option<&'static str>,
        events: Mutex<Vec<(&'static str, Option<usize>)>>,
    }

    impl Source {
        fn check(&self, stage: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.events
                .lock()
                .unwrap()
                .push((stage, Some(executor as *mut dyn Executor as *mut () as usize)));
            if self.reject == Some(stage) {
                return Err(Error::Forbidden(stage.into()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl NumberSource for Source {
        async fn operation(&self, executor: &mut dyn Executor) -> Result<()> {
            self.check("operation", executor)
        }
        async fn sales(&self, executor: &mut dyn Executor) -> Result<()> {
            self.check("sales", executor)
        }
        async fn evidence(&self, executor: &mut dyn Executor) -> Result<()> {
            self.check("evidence", executor)
        }
        async fn next(&self) -> Result<String> {
            self.events.lock().unwrap().push(("number", None));
            Ok("CA20261117-000001".into())
        }
    }

    #[tokio::test]
    async fn every_authorization_rejection_consumes_no_number() {
        for (index, stage) in ["operation", "sales", "evidence"].into_iter().enumerate() {
            let source = Source { reject: Some(stage), events: Mutex::new(Vec::new()) };
            let mut executor = SnapshotExecutor::default();
            let error = authorized_number(&source, &mut executor).await.unwrap_err();
            assert!(matches!(error, Error::Forbidden(message) if message == stage));
            let events = source.events.lock().unwrap();
            assert_eq!(events.len(), index + 1);
            assert!(events.iter().all(|(name, _)| *name != "number"));
        }
    }

    #[tokio::test]
    async fn authorized_creation_checks_one_executor_before_independent_number() {
        let source = Source { reject: None, events: Mutex::new(Vec::new()) };
        let mut executor = SnapshotExecutor::default();
        let expected = &mut executor as *mut SnapshotExecutor as usize;
        assert_eq!(authorized_number(&source, &mut executor).await.unwrap(), "CA20261117-000001");
        assert_eq!(
            *source.events.lock().unwrap(),
            vec![
                ("operation", Some(expected)),
                ("sales", Some(expected)),
                ("evidence", Some(expected)),
                ("number", None)
            ]
        );
    }
}
