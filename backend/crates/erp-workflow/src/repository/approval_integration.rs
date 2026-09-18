//! ERP 审批集成仓储：业务对象快照与通知 outbox。

mod outbox;
mod runtime_read;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
pub use outbox::ApprovalNotificationOutboxRepositoryExt;
use persistence_core::{Error, Executor, Repository, Result, mongo_ops};
pub use runtime_read::{
    ApprovalRuntimeReadPage, ApprovalRuntimeReadRepository, ApprovalRuntimeReadRow, ApprovalRuntimeReadScope,
    ApprovalRuntimeReadTypeScope,
};

use crate::entity::approval_integration::ApprovalSubjectSnapshot;

/// 从 facet `total` 段提取计数（`items`/`total` 两段式分页拼装的共用入口）。
///
/// 空页兜底为 0；负数或越界计数失败关闭。重复检查的 stage 定义仍保留在各自调用方。
///
/// # 参数
/// * `count` - facet `total` 段首行计数（`None` 表示空页）
/// * `scope` - 计数越界时的错误域标签
///
/// # 返回
/// 返回非负总数。
///
/// # 错误
/// 计数为负或超出 `u64` 时返回越界错误。
pub(crate) fn facet_total_or_empty(count: Option<i64>, scope: &'static str) -> Result<u64> {
    let Some(count) = count else {
        return Ok(0);
    };
    u64::try_from(count).map_err(|_| Error::EntityMetadataOutOfRange(scope))
}

/// 写入与实例一一对应的不可变业务对象快照。
#[allow(async_fn_in_trait)]
pub trait ApprovalSubjectSnapshotRepositoryExt {
    /// 插入启动时冻结的业务对象快照；写后不得再更新。
    ///
    /// # 错误
    /// 同一实例已有快照或 MongoDB 写入失败时返回错误。
    async fn create_immutable_snapshot(
        &self,
        snapshot: &ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()>;

    /// 按已授权对象批量读取提交快照；调用方仍须校验对象类型和审批版本。
    ///
    /// # 参数
    /// * `objects` - 单据类型和业务身份；使用现有类型/对象/版本复合索引
    /// * `executor` - 调用方事务执行器
    /// # 返回
    /// 返回未删除的匹配快照。
    /// # 错误
    /// 数据库读取失败时返回错误。
    async fn list_by_business_objects(
        &self,
        objects: &[(crate::entity::document_registry::DocumentType, String)],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalSubjectSnapshot>>;

    /// 按审批实例读取唯一快照。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn find_by_process_instance_id(
        &self,
        approval_process_instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalSubjectSnapshot>>;

    /// 按审批实例批量读取不可变业务对象快照。
    ///
    /// # 参数
    /// * `approval_process_instance_ids` - 当前列表页内的审批实例 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的快照；调用方按实例 ID 关联。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn find_by_process_instance_ids(
        &self,
        approval_process_instance_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalSubjectSnapshot>>;
}

impl ApprovalSubjectSnapshotRepositoryExt for Repository<'_, ApprovalSubjectSnapshot> {
    async fn create_immutable_snapshot(
        &self,
        snapshot: &ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), snapshot, executor).await
    }

    async fn list_by_business_objects(
        &self,
        objects: &[(crate::entity::document_registry::DocumentType, String)],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalSubjectSnapshot>> {
        if objects.is_empty() {
            return Ok(Vec::new());
        }
        let clauses = objects
            .iter()
            .map(|(kind, id)| doc! { "document_type": kind.as_str(), "business_object_id": id })
            .collect::<Vec<_>>();
        self.find_many(doc! { "$or": clauses, "deleted_at": NOT_DELETED_TIMESTAMP_BSON }, executor).await
    }

    async fn find_by_process_instance_id(
        &self,
        approval_process_instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalSubjectSnapshot>> {
        self.find_one(snapshot_by_process_instance_filter(approval_process_instance_id), executor).await
    }

    async fn find_by_process_instance_ids(
        &self,
        approval_process_instance_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalSubjectSnapshot>> {
        if approval_process_instance_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(snapshot_by_process_instances_filter(approval_process_instance_ids), executor).await
    }
}

fn snapshot_by_process_instance_filter(approval_process_instance_id: &str) -> Document {
    doc! { "approval_process_instance_id": approval_process_instance_id }
}

fn snapshot_by_process_instances_filter(approval_process_instance_ids: &[String]) -> Document {
    doc! {
        "approval_process_instance_id": {
            "$in": approval_process_instance_ids.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::snapshot_by_process_instances_filter;

    #[test]
    fn snapshot_batch_filter_uses_only_requested_instances() {
        let filter = snapshot_by_process_instances_filter(&["inst-1".to_string(), "inst-2".to_string()]);
        assert_eq!(
            filter,
            doc! {
                "approval_process_instance_id": {
                    "$in": ["inst-1", "inst-2"],
                }
            }
        );
    }
}
