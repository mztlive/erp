use crate::repository::owned::WorkItemRepository;
use entities::work_item::{WorkItem, WorkItemType};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};

/// 构造映射任务唯一责任查询的精确过滤文档（INT-R18）。
///
/// 与 [`Repository::list_for_master_mapping_task`] 相同的精确过滤（正式责任
/// 类型 + 对象类型 + 对象 ID），并显式写入未删除谓词以便单元测试锁定软删除
/// 排除语义。实际查询另取稳定排序后前两条，调用方不得依赖自然顺序。
///
/// # 参数
/// * `mapping_task_id` - 映射任务 ID
///
/// # 返回
/// 返回含正式责任类型、业务对象引用与未删除标记的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯过滤构造，不访问数据库；只匹配 `MASTER_MAPPING_TASK` 对象类型。
fn master_mapping_task_unique_filter(mapping_task_id: &str) -> Document {
    doc! {
        "work_item_type": WorkItemType::BusinessException.as_str(),
        "business_object_type": "MASTER_MAPPING_TASK",
        "business_object_id": mapping_task_id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回映射任务唯一责任有界读取的稳定排序与行数上限。
///
/// # 返回
/// 返回 `created_at` 升序、同值按 `id` 升序并截断前两条的查询选项。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯选项构造，不访问数据库；排序键与截断行数固定，调用方不得改写。
fn master_mapping_task_unique_options() -> FindOptions {
    FindOptions::builder()
        .sort(doc! { "created_at": 1, "id": 1 })
        .limit(2)
        .build()
}

impl<'a> WorkItemRepository<'a> {
    /// 查询映射任务关联的正式责任任务，按创建时间稳定排序。
    ///
    /// # 参数
    /// * `mapping_task_id` - 映射任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的正式任务；调用方必须校验责任事实唯一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `work_items` 集合，按业务对象引用过滤映射任务，不访问映射任务集合。
    pub async fn list_for_master_mapping_task(
        &self,
        mapping_task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "work_item_type": WorkItemType::BusinessException.as_str(),
                "business_object_type": "MASTER_MAPPING_TASK",
                "business_object_id": mapping_task_id,
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 按映射任务精确查找唯一正式责任任务的有界读取（INT-R18）。
    ///
    /// 用与 [`Self::list_for_master_mapping_task`] 相同的精确过滤（类型 +
    /// 对象类型 + 对象 ID），但只取稳定排序后的前两条：零条由 Service 解释为
    /// 缺失，一条为唯一责任，两条即证明数据损坏。查询次数与任务数量无关。
    ///
    /// # 参数
    /// * `mapping_task_id` - 映射任务 ID
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
    /// 结论由 Service 解释为内部错误，本方法不裁决。
    pub async fn find_unique_for_master_mapping_task(
        &self,
        mapping_task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        mongo_ops::find_many(
            &self.collection(),
            master_mapping_task_unique_filter(mapping_task_id),
            master_mapping_task_unique_options(),
            executor,
        )
        .await
    }

    /// 按页面映射任务 ID 集合批量加载正式责任任务（INT-R17）。
    ///
    /// 一次 `$in` 查询装载本页全部任务关联的正式责任行，按映射任务 ID 归组
    /// 由 Service 解释；返回顺序为稳定排序，不承诺与输入一致。
    ///
    /// # 参数
    /// * `mapping_task_ids` - 本页映射任务 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的正式任务；缺项表示该映射任务尚无责任行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `work_items` 集合，不访问映射任务集合；不裁决唯一性。
    pub async fn list_for_master_mapping_tasks(
        &self,
        mapping_task_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        if mapping_task_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! {
                "work_item_type": WorkItemType::BusinessException.as_str(),
                "business_object_type": "MASTER_MAPPING_TASK",
                "business_object_id": { "$in": mapping_task_ids },
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{master_mapping_task_unique_filter, master_mapping_task_unique_options};
    use entities::work_item::WorkItemType;

    /// 映射任务唯一过滤：锁定正式责任类型、对象类型与软删除排除（INT-R18）。
    #[test]
    fn mapping_task_unique_filter_pins_formal_type_and_soft_delete() {
        let filter = master_mapping_task_unique_filter("task-1");
        assert_eq!(
            filter.get_str("work_item_type").unwrap(),
            WorkItemType::BusinessException.as_str()
        );
        assert_eq!(
            filter.get_str("business_object_type").unwrap(),
            "MASTER_MAPPING_TASK"
        );
        assert_eq!(filter.get_str("business_object_id").unwrap(), "task-1");
        assert_eq!(
            filter.get_i64("deleted_at").unwrap(),
            entity_core::NOT_DELETED_TIMESTAMP as i64
        );
    }

    /// 唯一读取选项：稳定排序后截断前两条（INT-R18 两条即证损坏）。
    #[test]
    fn mapping_task_unique_options_bound_two_rows_with_stable_sort() {
        let options = master_mapping_task_unique_options();
        assert_eq!(options.sort.unwrap(), doc! { "created_at": 1, "id": 1 });
        assert_eq!(options.limit.unwrap(), 2);
    }
}
