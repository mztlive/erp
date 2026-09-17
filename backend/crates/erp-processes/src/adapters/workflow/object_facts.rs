//! 只读事实委派唯一读模型；跨域责任写入仍在流程适配器。

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_workflow::Result as WorkflowResult;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::ports::{ObjectFactKey, ObjectFactMap, ObjectFactPort, W29CloseFact};
use mongodb::Database;
use persistence_core::Executor;

use super::{map_service, purchase_responsibility, w29_close};
use crate::errors::Error;

/// Domain-object facts consumed by work-item commands.
#[derive(Clone)]
pub struct WorkflowObjectFacts {
    db: Database,
    reader: WorkItemFactsReader,
}
impl WorkflowObjectFacts {
    /// Bind domain repositories used by work-item authorization without reading them.
    pub fn new(db: Database) -> Self {
        Self { reader: WorkItemFactsReader::new(db.clone()), db }
    }
    fn db(&self) -> &Database {
        &self.db
    }
}
#[async_trait]
impl ObjectFactPort for WorkflowObjectFacts {
    async fn load_object_facts(
        &self,
        keys: &HashSet<ObjectFactKey>,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<ObjectFactMap> {
        self.reader.load(keys, executor).await.map_err(|error| map_service(Error::from(error)))
    }
    async fn counterparty_is_active(
        &self,
        kind: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        self.reader
            .counterparty_is_active(kind, id, executor)
            .await
            .map_err(|error| map_service(Error::from(error)))
    }
    async fn counterparty_numbers(
        &self,
        kind: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> WorkflowResult<HashMap<String, String>> {
        self.reader
            .counterparty_numbers(kind, ids, executor)
            .await
            .map_err(|error| map_service(Error::from(error)))
    }
    async fn external_identity_map_exists(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<bool> {
        self.reader.external_identity_map_exists(id, executor).await.map_err(erp_workflow::Error::from)
    }
    async fn assignment_separation_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Vec<String>> {
        let _ = (item, executor);
        Ok(Vec::new())
    }
    async fn purchase_order_fulfillment_scope(
        &self,
        selected: &WorkItem,
        purchase_order_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<(String, Vec<WorkItem>)> {
        purchase_responsibility::purchase_order_fulfillment_scope(
            self.db(),
            selected,
            purchase_order_id,
            executor,
        )
        .await
    }
    async fn reassign_purchase_order_owner(
        &self,
        purchase_order_id: &str,
        target_user_id: &str,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        purchase_responsibility::reassign_purchase_order_owner(
            self.db(),
            purchase_order_id,
            target_user_id,
            actor_id,
            executor,
        )
        .await
    }
    async fn reassign_integration_handler(
        &self,
        item: &mut WorkItem,
        target_user_id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        super::w29_reassign::reassign_handler(self.db(), item, target_user_id, executor).await
    }
    fn prepare_w29_close(
        &self,
        reason_code: &str,
        comment: Option<&str>,
        replacement_work_item_id: Option<&str>,
    ) -> WorkflowResult<W29CloseFact> {
        w29_close::prepare_w29_close(reason_code, comment, replacement_work_item_id)
    }
    async fn persist_w29_close(
        &self,
        item: &WorkItem,
        decision: &W29CloseFact,
        evidence_reference: &str,
        actor_id: &str,
        receipt_id: &str,
        closed_at: Instant,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        w29_close::persist_w29_close(
            self.db(),
            w29_close::W29CloseInput { item, decision, evidence_reference, actor_id, receipt_id, closed_at },
            executor,
        )
        .await
    }
}
