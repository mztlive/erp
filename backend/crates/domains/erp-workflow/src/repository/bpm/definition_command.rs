use bpm::model::types::ApprovalDefinitionStatus;
use bpm::model::{ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition};
use mongodb::bson::doc;
use persistence_core::{Executor, Result, mongo_ops};

use super::{BpmWorkflowRepository, CasWriteOutcome, NODE_DEFINITIONS, TRANSITION_DEFINITIONS};

impl<'a> BpmWorkflowRepository<'a> {
    /// 以 `id + DRAFT + expected_definition_lock_version` 整组替换草稿图。
    ///
    /// # 参数
    /// * `definition` - 待写回的草稿定义。
    /// * `nodes` - 替换后的节点定义。
    /// * `transitions` - 替换后的连线定义。
    /// * `expected_definition_lock_version` - 草稿定义的预期锁版本。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 定义 CAS 未应用时原样返回该分类且不改子图；应用成功后返回同一 [`CasWriteOutcome::Applied`]。
    ///
    /// # 错误
    /// 元数据越界、序列化失败或 MongoDB 写入失败时返回错误。
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
        self.replace_graph_docs(&definition.base.id, nodes, transitions, executor).await?;
        Ok(outcome)
    }

    /// 先退役旧发布版本，再把草稿发布为当前唯一 `PUBLISHED`。
    ///
    /// # 参数
    /// * `definition` - 待发布的草稿定义。
    /// * `previous` - 需要退役的当前发布版本；没有时跳过退役。
    /// * `expected_definition_lock_version` - 草稿的预期锁版本。
    /// * `expected_previous_lock_version` - 旧发布的预期锁版本；`None` 时用 `previous` 自身的锁版本。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 旧发布 CAS 未应用时返回该分类且不发布草稿；否则返回草稿发布的 CAS 分类。
    ///
    /// # 错误
    /// 元数据越界、序列化失败或 MongoDB 写入失败时返回错误。
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
                .cas_write_definition(previous, expected, &[ApprovalDefinitionStatus::Published], executor)
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

    /// 在调用方执行器中按原顺序替换节点与连线，空批次保持直接成功语义。
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
            &self.db.collection::<ApprovalTransitionDefinition>(TRANSITION_DEFINITIONS),
            filter,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<ApprovalNodeDefinition>(NODE_DEFINITIONS),
            nodes,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<ApprovalTransitionDefinition>(TRANSITION_DEFINITIONS),
            transitions,
            executor,
        )
        .await
    }
}
