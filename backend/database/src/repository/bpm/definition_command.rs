use bpm::model::types::ApprovalDefinitionStatus;
use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition};
use mongodb::bson::doc;

use super::{BpmWorkflowRepository, CasWriteOutcome, NODE_DEFINITIONS, TRANSITION_DEFINITIONS};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

impl<'a> BpmWorkflowRepository<'a> {
    /// 以 `id + DRAFT + expected_definition_lock_version` 更新草稿定义字段。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn update_draft_definition(
        &self,
        definition: &ApprovalProcessDefinition,
        expected_definition_lock_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        self.cas_write_definition(
            definition,
            expected_definition_lock_version,
            &[ApprovalDefinitionStatus::Draft],
            executor,
        )
        .await
    }

    /// 以 `id + DRAFT + expected_definition_lock_version` 整组替换草稿图。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn replace_draft_graph(
        &self,
        definition: &ApprovalProcessDefinition,
        nodes: &[ApprovalNodeDefinition],
        transitions: &[ApprovalTransitionDefinition],
        expected_definition_lock_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        let outcome = self
            .cas_write_definition(
                definition,
                expected_definition_lock_version,
                &[ApprovalDefinitionStatus::Draft],
                executor,
            )
            .await?;
        if !matches!(outcome, CasWriteOutcome::Applied(_)) {
            return Ok(outcome);
        }
        self.replace_graph_docs(&definition.base.id, nodes, transitions, executor)
            .await?;
        Ok(outcome)
    }

    /// 先退役旧发布版本，再把草稿发布为当前唯一 `PUBLISHED`。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn publish_and_retire_previous(
        &self,
        definition: &ApprovalProcessDefinition,
        previous: Option<&ApprovalProcessDefinition>,
        expected_definition_lock_version: u64,
        expected_previous_lock_version: Option<u64>,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        if let Some(previous) = previous {
            let expected = expected_previous_lock_version.unwrap_or(previous.definition_lock_version());
            let retired = self
                .cas_write_definition(
                    previous,
                    expected,
                    &[ApprovalDefinitionStatus::Published],
                    executor,
                )
                .await?;
            if !matches!(retired, CasWriteOutcome::Applied(_)) {
                return Ok(retired);
            }
        }
        self.cas_write_definition(
            definition,
            expected_definition_lock_version,
            &[ApprovalDefinitionStatus::Draft],
            executor,
        )
        .await
    }

    async fn replace_graph_docs(
        &self,
        definition_id: &str,
        nodes: &[ApprovalNodeDefinition],
        transitions: &[ApprovalTransitionDefinition],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let filter = doc! { "process_definition_id": definition_id };
        mongo_ops::delete_many(
            &self.db.collection::<ApprovalNodeDefinition>(NODE_DEFINITIONS),
            filter.clone(),
            executor,
        )
        .await?;
        mongo_ops::delete_many(
            &self
                .db
                .collection::<ApprovalTransitionDefinition>(TRANSITION_DEFINITIONS),
            filter,
            executor,
        )
        .await?;
        mongo_ops::insert_many(&self.db.collection(NODE_DEFINITIONS), nodes.to_vec(), executor).await?;
        mongo_ops::insert_many(
            &self.db.collection(TRANSITION_DEFINITIONS),
            transitions.to_vec(),
            executor,
        )
        .await
    }
}
