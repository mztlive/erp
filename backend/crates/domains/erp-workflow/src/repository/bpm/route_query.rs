//! 审批链条读取：按实例与轮次取得每个节点的最近执行。

use bpm::graph::MAX_DEFINITION_NODES;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::ApprovalNodeExecution;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use persistence_core::{Error, Executor, Result, aggregate};

use super::{BpmWorkflowRepository, EXECUTIONS};

impl BpmWorkflowRepository<'_> {
    /// 批量读取指定轮次各节点的最近执行，不受历史分页截断影响。
    ///
    /// # 参数
    /// * `instance_id` - 已授权的精确实例。
    /// * `round_no` - 实例当前轮次。
    /// * `executor` - 调用方数据访问执行器。
    ///
    /// # 返回
    /// 每个节点至多一条执行；额外保留一条用于调用方检测损坏数据。
    ///
    /// # 错误
    /// 数据库聚合或执行反序列化失败时返回错误。
    pub async fn latest_node_executions(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        round_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        let limit = i64::try_from(MAX_DEFINITION_NODES + 1)
            .map_err(|_| Error::EntityMetadataOutOfRange("approval route node limit"))?;
        let pipeline = vec![
            doc! { "$match": {
                "process_instance_id": instance_id.as_ref(),
                "round_no": i64::from(round_no),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            } },
            doc! { "$sort": { "execution_no": -1 } },
            doc! { "$group": { "_id": "$node_key", "execution": { "$first": "$$ROOT" } } },
            doc! { "$replaceRoot": { "newRoot": "$execution" } },
            doc! { "$limit": limit },
        ];
        let collection = self.db.collection::<ApprovalNodeExecution>(EXECUTIONS);
        aggregate::rows(&collection, pipeline, executor).await
    }
}
