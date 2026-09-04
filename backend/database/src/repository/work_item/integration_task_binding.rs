//! 集成任务唯一责任有界读取（INT-R25）。
//!
//! 三处 Service 调用点（错误任务详情、差异详情、直接差异决定的存在性门禁）
//! 此前以无界 `find_many` 装载完整责任集合再判定唯一性或存在性；本模块提供
//! 稳定排序下至多取两条的有界精确读取，零条由 Service 解释为缺失、一条为唯一
//! 责任、两条即证明数据损坏。存在性门禁复用同一有界读取，非空即拒绝。

use entities::work_item::WorkItem;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use crate::executor::Executor;
use crate::repository::Repository;
use crate::{mongo_ops, Result};

/// 构造集成错误任务唯一责任查询的精确过滤文档。
///
/// 业务对象范围与旧无界读取一致（对象类型 + 对象 ID）；未删除谓词在旧实现
/// 中由基类 `find_many` 追加，此处显式写入过滤文档以便单元测试锁定软删除
/// 排除语义——软删除行按设计不命中。实际查询另取稳定排序后前两条，调用方不得依赖自然顺序。
///
/// # 参数
/// * `task_id` - 集成错误任务 ID
///
/// # 返回
/// 返回含业务对象引用与未删除标记的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯过滤构造，不访问数据库；只匹配 `integration_error_task` 对象类型，
/// 其他对象类型的责任行即使同 ID 也不命中。
fn integration_error_task_unique_filter(task_id: &str) -> Document {
    doc! {
        "business_object_type": "integration_error_task",
        "business_object_id": task_id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构造对账差异唯一责任查询的精确过滤文档。
///
/// 业务对象范围与旧无界读取一致（对象类型 + 对象 ID）；未删除谓词在旧实现
/// 中由基类 `find_many` 追加，此处显式写入过滤文档以便单元测试锁定软删除
/// 排除语义——软删除行按设计不命中。实际查询另取稳定排序后前两条，调用方不得依赖自然顺序。
///
/// # 参数
/// * `difference_id` - 对账差异 ID
///
/// # 返回
/// 返回含业务对象引用与未删除标记的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯过滤构造，不访问数据库；只匹配 `reconciliation_difference` 对象类型，
/// 其他对象类型的责任行即使同 ID 也不命中。
fn reconciliation_difference_unique_filter(difference_id: &str) -> Document {
    doc! {
        "business_object_type": "reconciliation_difference",
        "business_object_id": difference_id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回唯一责任有界读取的稳定排序与行数上限。
///
/// # 返回
/// 返回 `created_at` 升序、同值按 `id` 升序并截断前两条的查询选项。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯选项构造，不访问数据库；排序键与截断行数固定，调用方不得改写。
fn unique_read_options() -> FindOptions {
    FindOptions::builder()
        .sort(doc! { "created_at": 1, "id": 1 })
        .limit(2)
        .build()
}

impl<'a> Repository<'a, WorkItem> {
    /// 按集成错误任务精确查找唯一正式责任任务的有界读取（INT-R25）。
    ///
    /// 业务对象范围与旧无界读取一致（对象类型 + 对象 ID，同样排除软删除行），
    /// 但只取稳定排序后的前两条：零条由 Service 解释为缺失，一条为
    /// 唯一责任，两条即证明数据损坏。查询次数与责任行数量无关。
    ///
    /// # 参数
    /// * `task_id` - 集成错误任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回至多两条正式任务（稳定排序 `created_at`/`id`）；空集合表示无责任任务。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只返回实体，不返回 services DTO、HTTP View 或授权结论；大于一条的损坏
    /// 结论由 Service 解释为冲突错误，本方法不裁决；不开启或提交事务。
    pub async fn find_unique_for_integration_error_task(
        &self,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        mongo_ops::find_many(
            &self.collection(),
            integration_error_task_unique_filter(task_id),
            unique_read_options(),
            executor,
        )
        .await
    }

    /// 按对账差异精确查找唯一正式责任任务的有界读取（INT-R25）。
    ///
    /// 业务对象范围与旧无界读取一致（对象类型 + 对象 ID，同样排除软删除行），
    /// 但只取稳定排序后的前两条：零条由 Service 解释为缺失，一条为
    /// 唯一责任，两条即证明数据损坏；直接差异决定的存在性门禁复用同一读取，
    /// 非空即拒绝。查询次数与责任行数量无关。
    ///
    /// # 参数
    /// * `difference_id` - 对账差异 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回至多两条正式任务（稳定排序 `created_at`/`id`）；空集合表示无责任任务。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只返回实体，不返回 services DTO、HTTP View 或授权结论；大于一条的损坏
    /// 结论由 Service 解释为冲突错误，本方法不裁决；不开启或提交事务。
    pub async fn find_unique_for_reconciliation_difference(
        &self,
        difference_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        mongo_ops::find_many(
            &self.collection(),
            reconciliation_difference_unique_filter(difference_id),
            unique_read_options(),
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        integration_error_task_unique_filter, reconciliation_difference_unique_filter, unique_read_options,
    };

    #[test]
    fn error_task_unique_filter_pins_object_type_and_soft_delete() {
        let filter = integration_error_task_unique_filter("task-1");
        assert_eq!(
            filter.get_str("business_object_type").unwrap(),
            "integration_error_task"
        );
        assert_eq!(filter.get_str("business_object_id").unwrap(), "task-1");
        assert_eq!(
            filter.get_i64("deleted_at").unwrap(),
            entity_core::NOT_DELETED_TIMESTAMP as i64
        );
    }

    #[test]
    fn difference_unique_filter_pins_object_type_and_soft_delete() {
        let filter = reconciliation_difference_unique_filter("diff-1");
        assert_eq!(
            filter.get_str("business_object_type").unwrap(),
            "reconciliation_difference"
        );
        assert_eq!(filter.get_str("business_object_id").unwrap(), "diff-1");
        assert_eq!(
            filter.get_i64("deleted_at").unwrap(),
            entity_core::NOT_DELETED_TIMESTAMP as i64
        );
    }

    #[test]
    fn unique_filters_reject_cross_subject_ids() {
        let error_filter = integration_error_task_unique_filter("diff-1");
        let difference_filter = reconciliation_difference_unique_filter("task-1");
        assert_ne!(
            error_filter.get_str("business_object_type").unwrap(),
            difference_filter.get_str("business_object_type").unwrap()
        );
        assert_eq!(error_filter.get_str("business_object_id").unwrap(), "diff-1");
        assert_eq!(difference_filter.get_str("business_object_id").unwrap(), "task-1");
    }

    #[test]
    fn unique_read_options_bound_two_rows_with_stable_sort() {
        let options = unique_read_options();
        assert_eq!(options.sort.unwrap(), doc! { "created_at": 1, "id": 1 });
        assert_eq!(options.limit.unwrap(), 2);
    }
}
