//! 域 D03 `work_item` 仓储：work_item。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 「领取 = 条件更新（行锁）」原子入口（数据模型 §6.1）。集合名常量统一从
//! `extensions::WorkItemExt` 关联常量导入（conventions §4.3）。
//!
//! 筛选/行类型定义在本文件，经 `WorkItemExt` 的关联类型对外暴露。

use entities::work_item::{WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType};
use entity_core::{HasBaseModel, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

/// 待办列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemRow {
    /// 实体主键。
    pub id: String,
    /// 任务类型。
    pub work_item_type: WorkItemType,
    /// 业务对象类型代码。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 任务针对的对象版本。
    pub subject_version: Option<String>,
    /// 任务状态。
    pub status: WorkItemStatus,
    /// 责任角色。
    pub owner_role: Option<String>,
    /// 当前责任人。
    pub owner_user_id: Option<String>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限（秒级时间戳）。
    pub due_at: Option<u64>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 待办列表筛选条件。
#[derive(Debug, Clone)]
pub struct WorkItemFilter {
    /// 任务类型；`None` 表示不筛选。
    pub work_item_type: Option<WorkItemType>,
    /// 任务状态；`None` 表示不筛选。
    pub status: Option<WorkItemStatus>,
    /// 责任角色；`None` 表示不筛选。
    pub owner_role: Option<String>,
    /// 当前责任人；`None` 表示不筛选。
    pub owner_user_id: Option<String>,
    /// 优先级；`None` 表示不筛选。
    pub priority: Option<WorkItemPriority>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at` / `due_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for WorkItemFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(work_item_type) = self.work_item_type {
            filter.insert("work_item_type", work_item_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(owner_role) = &self.owner_role {
            filter.insert("owner_role", owner_role);
        }
        if let Some(owner_user_id) = &self.owner_user_id {
            filter.insert("owner_user_id", owner_user_id);
        }
        if let Some(priority) = self.priority {
            filter.insert("priority", priority.as_str());
        }
        filter
    }
}

impl Pagination for WorkItemFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, WorkItem> {
    /// 分页检索待办列表（投影查询，工作队列）。
    ///
    /// 只返回 [`WorkItemRow`] 所需的列表字段，不加载整文档；`owner_role` /
    /// `owner_user_id` 精确匹配，组合覆盖 `idx_work_items_queue` 工作队列索引
    /// 前缀（§6.1）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_work_items(
        &self,
        filter: &WorkItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<WorkItemRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(work_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<WorkItemRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 领取任务（条件更新原子完成，行锁语义）。
    ///
    /// 数据模型 §6.1：领取 = 条件更新，仅当行内状态仍为 `UNCLAIMED` 时迁移到
    /// `IN_PROGRESS` 并写入领取人，同一时刻只能被一个用户处理，无需租约或令牌。
    /// 调用方应先通过 `WorkItem::claim` 做状态机校验（本方法不再校验内存状态），
    /// 然后调用本方法把迁移原子落库：过滤条件同时比较 `id + version + status`，
    /// 任一不匹配（他人已领取、版本陈旧）返回
    /// [`crate::Error::OptimisticLockingError`]，内存实体版本不被改写。
    ///
    /// 单条条件写，**不需要事务执行器**；传入 `NoTransaction` 行为可预期。
    ///
    /// # 参数
    /// * `item` - 已由实体 `claim` 迁移到 `IN_PROGRESS` 的任务（携带领取人）
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 无返回值；成功时同步递增内存实体 `version` / `updated_at`。
    ///
    /// # 错误
    /// 行内状态或版本不匹配时返回 [`crate::Error::OptimisticLockingError`]；
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn claim(&self, item: &mut WorkItem, executor: &mut dyn Executor) -> Result<()> {
        let base = item.base().clone();
        let expected_version =
            i64::try_from(base.version).map_err(|_| Error::EntityMetadataOutOfRange("version"))?;
        let next_version = base
            .version
            .checked_add(1)
            .ok_or(Error::EntityMetadataOutOfRange("version"))?;
        let updated_at_bson = chrono::Local::now().timestamp();
        let updated_at =
            u64::try_from(updated_at_bson).map_err(|_| Error::EntityMetadataOutOfRange("updated_at"))?;

        let result = mongo_ops::update_one(
            &self.collection(),
            doc! {
                "id": &base.id,
                "version": expected_version,
                "status": WorkItemStatus::Unclaimed.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            doc! {
                "$set": {
                    "status": WorkItemStatus::InProgress.as_str(),
                    "owner_user_id": item.owner_user_id.clone(),
                    "version": i64::try_from(next_version)
                        .map_err(|_| Error::EntityMetadataOutOfRange("version"))?,
                    "updated_at": updated_at_bson,
                }
            },
            false,
            executor,
        )
        .await?;
        if result.matched_count == 0 {
            return Err(Error::OptimisticLockingError);
        }
        item.base_mut().version = next_version;
        item.base_mut().updated_at = updated_at;
        Ok(())
    }

    /// 查询业务对象当前的全部有效任务（`UNCLAIMED` / `IN_PROGRESS`）。
    ///
    /// 数据模型 §6.1：同一业务对象、任务类型同时最多一个有效任务，唯一性由
    /// 部分唯一索引 `uk_work_items_active` 承担；本方法供服务层派发前核对
    /// 既有有效任务（如重复派发保护），查询条件与索引部分过滤表达式一致。
    ///
    /// # 参数
    /// * `business_object_type` - 业务对象类型代码
    /// * `business_object_id` - 业务对象 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该业务对象当前全部有效任务。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_active_by_object(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "business_object_type": business_object_type,
                "business_object_id": business_object_id,
                "status": { "$in": [
                    WorkItemStatus::Unclaimed.as_str(),
                    WorkItemStatus::InProgress.as_str(),
                ] },
            },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }
}

/// 构建排序文档（排序字段白名单化，禁止透传任意字段名）。
///
/// 仅允许 `created_at` / `updated_at` / `due_at`；未知字段回落默认 `created_at`。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或白名单外字段时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        Some("due_at") => "due_at",
        _ => "created_at",
    };
    doc! { field: direction }
}

/// 待办列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn work_item_projection() -> Document {
    doc! {
        "id": 1,
        "work_item_type": 1,
        "business_object_type": 1,
        "business_object_id": 1,
        "subject_version": 1,
        "status": 1,
        "owner_role": 1,
        "owner_user_id": 1,
        "priority": 1,
        "due_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, QueryFilter, WorkItemFilter};
    use entities::work_item::{WorkItemPriority, WorkItemStatus, WorkItemType};
    use mongodb::bson::doc;

    #[test]
    fn filter_applies_type_status_and_owner_fields() {
        let filter = WorkItemFilter {
            work_item_type: Some(WorkItemType::ImportBusinessConfirmation),
            status: Some(WorkItemStatus::InProgress),
            owner_role: Some("sales".to_string()),
            owner_user_id: Some("user-1".to_string()),
            priority: Some(WorkItemPriority::High),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(
            document.get_str("work_item_type").unwrap(),
            "IMPORT_BUSINESS_CONFIRMATION"
        );
        assert_eq!(document.get_str("status").unwrap(), "IN_PROGRESS");
        assert_eq!(document.get_str("owner_role").unwrap(), "sales");
        assert_eq!(document.get_str("owner_user_id").unwrap(), "user-1");
        assert_eq!(document.get_str("priority").unwrap(), "high");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("due_at"), true), doc! { "due_at": 1 });
        assert_eq!(
            sort_doc(Some("business_object_id"), false),
            doc! { "created_at": -1 },
            "白名单外字段回落默认排序"
        );
    }
}
