//! 将工作流审计写入与回执读取装配到审计域。

use application_core::CommandReceiptFact;
use async_trait::async_trait;
use erp_audit::repository::prelude::*;
use erp_audit::{AuditExt, AuditLog, AuditLogData};
use erp_core::AccountKind;
use erp_workflow::ports::{PreparedWorkflowAudit, WorkflowAuditFact, WorkflowAuditPort};
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;
use crate::errors::Error;

/// Persist workflow audits through `erp-audit`.
#[derive(Clone)]
pub struct WorkflowAudit {
    db: Database,
}

impl WorkflowAudit {
    /// Bind audit persistence to `db`.
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl WorkflowAuditPort for WorkflowAudit {
    async fn persist(
        &self,
        audit: &PreparedWorkflowAudit,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        let actor_type = AccountKind::parse(&audit.actor_type)
            .map_err(|error| WorkflowError::ValidationError(error.to_string()))?;
        let log = AuditLog::new(
            audit.id.clone(),
            AuditLogData {
                actor_id: audit.actor_id.clone(),
                actor_account: audit.actor_account.clone(),
                actor_type,
                action: audit.action.clone(),
                resource_type: audit.resource_type.clone(),
                resource_id: Some(audit.resource_id.clone()),
                success: true,
                message: audit.message.clone(),
            },
        )
        .map_err(|error| map_service(Error::from(error)))?;
        self.db.audit_logs().create(&log, executor).await.map_err(WorkflowError::from)?;
        Ok(())
    }

    async fn find_command_receipts_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Vec<CommandReceiptFact>> {
        self.db
            .audit_logs()
            .find_command_receipts_by_ids(ids, executor)
            .await
            .map_err(|error| map_service(Error::from(error)))
    }

    async fn list_successful_resource_audits(
        &self,
        resource_type: &str,
        resource_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Vec<WorkflowAuditFact>> {
        Ok(self
            .db
            .audit_logs()
            .list_successful_by_resource(resource_type, resource_id, executor)
            .await
            .map_err(|error| map_service(Error::from(error)))?
            .into_iter()
            .map(|log| WorkflowAuditFact {
                actor_id: log.actor_id,
                action: log.action,
                resource_type: log.resource_type,
                resource_id: log.resource_id,
                success: log.success,
                message: log.message,
            })
            .collect())
    }
}
