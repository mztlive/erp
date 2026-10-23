//! 读取精确实例的冻结运行图。
use bpm::model::{ApprovalInstanceAssignee, ApprovalProcessInstance};
use mongodb::bson::doc;
use persistence_core::{Error, Executor, Repository, Result};

use super::{ASSIGNEES, BpmWorkflowRepository, DefinitionGraph};

impl BpmWorkflowRepository<'_> {
    /// 读取绑定定义及实例全部节点指派，恢复动态节点的启动快照。
    /// # 参数
    /// 精确实例与调用方执行器。
    /// # 返回
    /// 定义不存在返回空；存在时返回完整运行图。
    /// # 错误
    /// 来源或版本不一致、指派缺失或数据库错误时拒绝。
    pub async fn load_instance_graph(
        &self,
        instance: &ApprovalProcessInstance,
        ex: &mut dyn Executor,
    ) -> Result<Option<DefinitionGraph>> {
        let Some(graph) = self.load_definition_graph(&instance.process_definition_id, ex).await? else {
            return Ok(None);
        };
        if graph.nodes.iter().all(|node| node.assignee_rule.is_none()) {
            return Ok(Some(graph));
        }
        let assignments = Repository::<ApprovalInstanceAssignee>::new(self.db, ASSIGNEES)
            .find_many(doc! { "process_instance_id": instance.base.id.clone() }, ex)
            .await?;
        graph
            .with_frozen_assignees(instance, &assignments)
            .map(Some)
            .map_err(|_| Error::EntityMetadataOutOfRange("dynamic approval assignment"))
    }
}
