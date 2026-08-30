//! 域 D03 `work_item` 仓储：指定责任人的人工任务队列查询。

use std::num::NonZeroU32;

use bpm::ApprovalNodeExecutionId;
use entities::common::time::Instant;
use entities::work_item::{
    AssignmentSource, FinanceResponsibilityOperation, FinanceResponsibilityRule, WorkItem, WorkItemPriority,
    WorkItemStatus, WorkItemType,
};
use entity_core::{HasBaseModel, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{doc, serialize_to_document, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::bpm::{approval_task_cas_filter, classify_cas_miss, CasWriteOutcome};
use super::{Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

/// 队列列表的最小任务事实投影。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemRow {
    /// 任务 ID。
    pub id: String,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 类型化审批节点执行；审批任务存在，独立任务为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_node_execution_id: Option<String>,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被处理的业务版本。
    pub subject_version: String,
    /// 生命周期状态。
    pub status: WorkItemStatus,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: Option<String>,
    /// 曾形成个人责任的用户 ID；仅供服务端历史范围过滤。
    pub responsibility_actor_ids: Vec<String>,
    /// 当前或最近责任来源。
    pub assignment_source: AssignmentSource,
    /// 首次形成个人责任的时间。
    pub assigned_at: Option<Instant>,
    /// 首次正式处理时间。
    pub started_at: Option<Instant>,
    /// 当前个人责任生效时间。
    pub current_assignment_at: Option<Instant>,
    /// 最近一次活动时间。
    pub last_activity_at: Option<Instant>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 影响摘要。
    pub impact_summary: Option<String>,
    /// 正式完成时间。
    pub completed_at: Option<Instant>,
    /// 正式完成人。
    pub completed_by: Option<String>,
    /// 关闭时间。
    pub closed_at: Option<Instant>,
    /// 关闭操作人。
    pub closed_by: Option<String>,
    /// 关闭原因。
    pub close_reason: Option<String>,
    /// 持久化乐观锁版本；API 必须映射为 `task_version`。
    pub version: u64,
    /// 创建时间。
    pub created_at: u64,
    /// 最近持久化更新时间。
    pub updated_at: u64,
}

/// 待办列表筛选条件。
#[derive(Debug, Clone, Default)]
pub struct WorkItemFilter {
    /// 允许的任务类型集合；为空时不筛选。
    pub work_item_types: Vec<WorkItemType>,
    /// 允许的状态集合；为空时不筛选。
    pub statuses: Vec<WorkItemStatus>,
    /// 允许的责任组织集合；为空时不筛选。
    pub owner_organization_ids: Vec<String>,
    /// 允许的注册任务类型与权威业务对象类型组合。
    pub object_access_shapes: Option<Vec<(WorkItemType, String)>>,
    /// 当前个人责任人；为空时不筛选。
    pub owner_user_id: Option<String>,
    /// 历史参与人；匹配曾负责、完成人或关闭人之一。
    pub history_actor_id: Option<String>,
    /// 具备组织级历史查看权的组织集合；`Some(空)` 表示公司级。
    pub history_managed_organization_ids: Option<Vec<String>>,
    /// 到期时间下界（包含）。
    pub due_from: Option<Instant>,
    /// 到期时间上界（不包含）。
    pub due_before: Option<Instant>,
    /// 允许的优先级集合；为空时不筛选。
    pub priorities: Vec<WorkItemPriority>,
    /// 权限过滤范围内的安全字面量检索。
    pub query: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段白名单值。
    pub sort_by: Option<String>,
    /// 是否升序。
    pub sort_ascending: bool,
}

impl QueryFilter for WorkItemFilter {
    /// 构造与责任队列索引一致的 MongoDB 查询条件。
    ///
    /// # 返回
    /// 返回包含软删除约束的查询文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_enum_filter(&mut filter, "status", &self.statuses, WorkItemStatus::as_str);
        insert_enum_filter(
            &mut filter,
            "work_item_type",
            &self.work_item_types,
            WorkItemType::as_str,
        );
        insert_enum_filter(
            &mut filter,
            "priority",
            &self.priorities,
            WorkItemPriority::as_str,
        );
        insert_string_filter(&mut filter, "owner_organization_id", &self.owner_organization_ids);
        if let Some(owner_user_id) = &self.owner_user_id {
            filter.insert("owner_user_id", owner_user_id);
        }
        let mut conjunctions = Vec::new();
        if let Some(shapes) = &self.object_access_shapes {
            conjunctions.push(object_access_shape_filter(shapes));
        }
        if self.history_actor_id.is_some() || self.history_managed_organization_ids.is_some() {
            conjunctions.push(history_scope_filter(
                self.history_actor_id.as_deref(),
                self.history_managed_organization_ids.as_deref(),
            ));
        }
        if let Some(query) = self.query.as_deref() {
            conjunctions.push(literal_query_filter(query));
        }
        if !conjunctions.is_empty() {
            filter.insert("$and", conjunctions);
        }
        insert_due_range(&mut filter, self.due_from, self.due_before);
        filter
    }
}

impl Pagination for WorkItemFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)`。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, WorkItem> {
    /// 按主键读取单据审批任务。
    ///
    /// # 参数
    /// * `id` - 待办任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的单据审批任务；不存在或类型不符时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本查询固定限制 `DOCUMENT_APPROVAL`，禁止运行时命令误消费其它任务类型。
    pub async fn find_document_approval_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "id": id,
                "work_item_type": WorkItemType::DocumentApproval.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 统计指定节点执行当前开放的单据审批任务。
    ///
    /// # 参数
    /// * `execution_id` - 审批节点执行 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回开放审批任务数量。
    ///
    /// # 错误
    /// MongoDB 统计失败时返回错误。
    ///
    /// # 关键业务约束
    /// 统计固定限制任务类型、开放状态与未删除标记，供 BPM 开放任务不变量校验。
    pub async fn count_open_document_approval_by_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        mongo_ops::count_documents(
            &self.collection(),
            doc! {
                "approval_node_execution_id": execution_id.as_ref(),
                "work_item_type": WorkItemType::DocumentApproval.as_str(),
                "status": WorkItemStatus::Open.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 查询指定账号当前开放的单据审批任务。
    ///
    /// # 参数
    /// * `owner_user_id` - 当前责任人账号 ID
    /// * `business_object_type` - 可选业务对象类型稳定码
    /// * `limit` - 最大返回条数；为零时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按创建时间升序排列的开放单据审批任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 查询固定限制当前责任人、`DOCUMENT_APPROVAL` 与 `OPEN`，不得返回历史任务。
    pub async fn list_open_document_approval_owned_by(
        &self,
        owner_user_id: &str,
        business_object_type: Option<&str>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut filter = doc! {
            "owner_user_id": owner_user_id,
            "work_item_type": WorkItemType::DocumentApproval.as_str(),
            "status": WorkItemStatus::Open.as_str(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        if let Some(business_object_type) = business_object_type {
            filter.insert("business_object_type", business_object_type);
        }
        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1, "id": 1 })
            .limit(i64::from(limit))
            .build();
        mongo_ops::find_many(&self.collection(), filter, options, executor).await
    }

    /// 按固定批次读取队列候选任务投影。
    ///
    /// 本方法不执行未授权候选总数统计；Service 必须逐批加载权威
    /// 业务对象事实，完成参与权过滤后再形成分页和总数。
    ///
    /// # 参数
    /// * `filter` - 服务端责任范围与业务筛选
    /// * `offset` - 未授权候选集的起始偏移
    /// * `batch_size` - 非零固定批次大小
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前候选批次，不暴露仓储候选总数为授权总数。
    ///
    /// # 错误
    /// MongoDB 查询、游标读取或反序列化失败时返回错误。
    pub async fn scan_work_item_batch(
        &self,
        filter: &WorkItemFilter,
        offset: u64,
        batch_size: NonZeroU32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(offset)
            .limit(i64::from(batch_size.get()))
            .projection(work_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<WorkItemRow>();
        mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await
    }

    /// 在与队列完全相同的授权筛选内查找焦点任务。
    ///
    /// # 返回
    /// 任务同时满足 ID 与全部 scope/角色/组织/业务筛选时返回完整实体；否则
    /// 返回 `None`，调用方不得退回无过滤的 ID 查询。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_visible_by_id(
        &self,
        id: &str,
        filter: &WorkItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        let mut document = filter.to_doc();
        document.insert("id", id);
        mongo_ops::find_one(&self.collection(), document, executor).await
    }

    /// 批量查询多个业务对象当前开放的审批任务。
    ///
    /// 查询使用业务对象类型与 ID 的精确组合，避免不同单据类型之间形成交叉命中；
    /// 只返回 `DOCUMENT_APPROVAL + OPEN` 任务。
    ///
    /// # 参数
    /// * `business_objects` - `(业务对象类型, 业务对象 ID)` 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间升序排列的开放审批任务；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_active_approval_by_objects(
        &self,
        business_objects: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        if business_objects.is_empty() {
            return Ok(Vec::new());
        }
        let object_filters = business_objects
            .iter()
            .map(|(object_type, object_id)| {
                doc! {
                    "business_object_type": object_type,
                    "business_object_id": object_id,
                }
            })
            .collect::<Vec<_>>();
        self.find_many_sorted(
            doc! {
                "work_item_type": WorkItemType::DocumentApproval.as_str(),
                "status": WorkItemStatus::Open.as_str(),
                "$or": object_filters,
            },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }

    /// 读取指定节点执行当前关联的开放审批任务。
    ///
    /// 查询同时约束 `DOCUMENT_APPROVAL + OPEN + approval_node_execution_id`，并按
    /// 创建时间升序返回全部命中，用于调用方识别零任务或重复任务的不一致事实。
    ///
    /// # 参数
    /// * `execution_id` - 当前审批节点执行
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回指定执行关联的全部开放审批任务。
    ///
    /// # 错误
    /// MongoDB 查询、游标读取或反序列化失败时返回错误。
    pub async fn open_approval_tasks_for_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            open_approval_execution_filter(execution_id),
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }

    /// 读取指定节点执行关联的全部审批任务。
    ///
    /// 本查询不限制任务状态，供人员恢复命令核对旧任务已经关闭且版本未漂移；
    /// 调用方不得用本接口重新打开或修改历史任务。
    ///
    /// # 参数
    /// * `execution_id` - 原受阻节点执行
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间升序排列的全部单据审批任务。
    ///
    /// # 错误
    /// MongoDB 查询、游标读取或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 人员恢复必须创建绑定新执行的新任务，不得把本查询返回的旧任务改回开放状态。
    pub async fn approval_tasks_for_execution(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "approval_node_execution_id": execution_id.as_ref(),
                "work_item_type": WorkItemType::DocumentApproval.as_str(),
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 持久化已由实体规则形成的审批取消关闭任务。
    ///
    /// 每条任务继续使用加载时版本和节点执行引用执行 `OPEN` CAS；调用方必须传入
    /// 同一事务执行器，保证任务关闭与 BPM 运行事实、业务单据写回原子提交。
    ///
    /// # 参数
    /// * `items` - 已由 `WorkItem::close_all_for_approval_cancellation` 关闭的任务快照
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 全部任务 CAS 写入成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务缺少节点执行引用、版本溢出、CAS 未命中或 MongoDB 写入失败时返回错误。
    pub async fn persist_cancelled_approval_tasks(
        &self,
        items: &[WorkItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for item in items {
            let execution_id = item
                .approval_node_execution_id
                .as_ref()
                .ok_or(Error::EntityMetadataOutOfRange("approval_node_execution_id"))?;
            let outcome = self
                .close_approval_task(item, item.base.version, execution_id, executor)
                .await?;
            if !matches!(outcome, CasWriteOutcome::Applied(_)) {
                return Err(Error::OptimisticLockingError);
            }
        }
        Ok(())
    }

    /// 以 `id + OPEN + expected_task_version + approval_node_execution_id` 关闭审批任务。
    ///
    /// 改派和人员恢复不得更新旧 `CLOSED` 任务，只能为新执行插入新任务。
    ///
    /// # 参数
    /// * `item` - 已完成实体状态变更的审批任务
    /// * `expected_task_version` - 加载时任务版本
    /// * `approval_node_execution_id` - 任务绑定的节点执行
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回应用、缺失、版本冲突或状态变化的 CAS 分类。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 更新失败时返回错误。
    pub async fn close_approval_task(
        &self,
        item: &WorkItem,
        expected_task_version: u64,
        approval_node_execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<WorkItem>> {
        self.persist_open_approval_task(item, expected_task_version, approval_node_execution_id, executor)
            .await
    }

    /// 查询业务对象当前全部开放任务。
    ///
    /// 该查询供强类型服务核对当前责任事实；同一对象与任务类型的开放唯一性由
    /// `uk_work_items_open_object_type` 部分唯一索引保证。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
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
                "status": WorkItemStatus::Open.as_str(),
            },
            doc! { "created_at": 1 },
            executor,
        )
        .await
    }

    /// 查询同一责任键下全部开放采购履约任务。
    ///
    /// # 参数
    /// * `responsibility_key` - 采购单责任键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间、任务 ID 稳定排序的开放履约任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 查询固定限制 `FULFILLMENT_OPERATION + OPEN`，供采购单责任转交原子级联。
    pub async fn list_open_fulfillment_by_responsibility_key(
        &self,
        responsibility_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "work_item_type": WorkItemType::FulfillmentOperation.as_str(),
                "responsibility_key": responsibility_key,
                "status": WorkItemStatus::Open.as_str(),
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 查询具体账号拥有的开放供给分配任务。
    ///
    /// # 参数
    /// * `owner_user_id` - 当前已认证采购账号
    /// * `sales_order_id` - 可选来源销售单筛选
    /// * `work_item_id` - 可选任务 ID 筛选
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按创建时间升序排列的开放供给分配任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_open_procurement_owned_by(
        &self,
        owner_user_id: &str,
        sales_order_id: Option<&str>,
        work_item_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        let mut filter = doc! {
            "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
            "business_object_type": "sales_order",
            "status": WorkItemStatus::Open.as_str(),
            "owner_user_id": owner_user_id,
        };
        if let Some(sales_order_id) = sales_order_id {
            filter.insert("business_object_id", sales_order_id);
        }
        if let Some(work_item_id) = work_item_id {
            filter.insert("id", work_item_id);
        }
        self.find_many_sorted(filter, doc! { "created_at": 1 }, executor)
            .await
    }

    /// 查询销售单全部供给分配任务并把最新任务排在前面。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单稳定身份
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按更新时间、创建时间倒序排列的全部生命周期任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_by_sales_order_newest_first(
        &self,
        sales_order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "business_object_type": "sales_order",
                "business_object_id": sales_order_id,
                "work_item_type": WorkItemType::ProcurementOrderCreation.as_str(),
            },
            doc! { "updated_at": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    /// 查询应付子账全部付款执行任务并把最新任务排在前面。
    ///
    /// # 参数
    /// * `payable_account_id` - 应付子账稳定身份
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按更新时间、创建时间倒序排列的全部生命周期任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_payment_execution_by_payable_newest_first(
        &self,
        payable_account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "business_object_type": "payable_account",
                "business_object_id": payable_account_id,
                "work_item_type": WorkItemType::SupplierPaymentExecution.as_str(),
            },
            doc! { "updated_at": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    /// 查询应收子账全部销项开票执行任务并把最新任务排在前面。
    ///
    /// # 参数
    /// * `receivable_account_id` - 应收子账稳定身份
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按更新时间、创建时间倒序排列的全部生命周期任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_sales_invoice_execution_by_receivable_newest_first(
        &self,
        receivable_account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "business_object_type": "receivable_account",
                "business_object_id": receivable_account_id,
                "work_item_type": WorkItemType::SalesInvoiceExecution.as_str(),
            },
            doc! { "updated_at": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    async fn persist_open_approval_task(
        &self,
        item: &WorkItem,
        expected_task_version: u64,
        approval_node_execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<WorkItem>> {
        let next_version = next_task_version(expected_task_version)?;
        let mut set_doc = serialize_to_document(item)?;
        set_doc.insert("version", next_version);
        let matched = mongo_ops::update_one(
            &self.collection(),
            approval_task_cas_filter(&item.base.id, expected_task_version, approval_node_execution_id)?,
            doc! { "$set": set_doc },
            false,
            executor,
        )
        .await?
        .matched_count;
        if matched > 0 {
            let mut applied = item.clone();
            applied.base_mut().version = expected_task_version.saturating_add(1);
            return Ok(CasWriteOutcome::Applied(applied));
        }
        let current = self.find_by_id(&item.base.id, executor).await?;
        let expected_execution = approval_node_execution_id.clone();
        Ok(classify_cas_miss(current, expected_task_version, move |row| {
            approval_task_still_open(row, &expected_execution)
        }))
    }
}

impl<'a> Repository<'a, FinanceResponsibilityRule> {
    /// 查询全部未删除财务责任规则。
    ///
    /// # 返回
    /// 返回按业务、匹配层级和创建时间稳定排序的规则。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_finance_responsibility_rules(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinanceResponsibilityRule>> {
        self.find_many_sorted(
            doc! {},
            doc! { "operation": 1, "scope": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 查询指定业务全部启用财务责任规则。
    ///
    /// # 参数
    /// * `operation` - 供应商付款或销项开票
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前未删除且启用的规则。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_active_finance_responsibility_rules(
        &self,
        operation: FinanceResponsibilityOperation,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinanceResponsibilityRule>> {
        self.find_many_sorted(
            doc! {
                "operation": operation.as_str(),
                "status": entities::catalog::EnableStatus::Active.as_str(),
            },
            doc! { "scope": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }
}

/// 构造按节点执行读取开放审批任务的索引友好过滤条件。
///
/// # 参数
/// * `execution_id` - 当前审批节点执行
///
/// # 返回
/// 返回固定约束任务类型、开放状态和节点执行的查询文档。
///
/// # 错误
/// 无。
fn open_approval_execution_filter(execution_id: &ApprovalNodeExecutionId) -> Document {
    doc! {
        "work_item_type": WorkItemType::DocumentApproval.as_str(),
        "approval_node_execution_id": execution_id.as_ref(),
        "status": WorkItemStatus::Open.as_str(),
    }
}

/// 计算审批任务 CAS 的下一持久化版本。
///
/// # 参数
/// * `expected_task_version` - 加载时任务版本
///
/// # 返回
/// 返回可写入 BSON 的下一版本。
///
/// # 错误
/// 版本溢出或无法表示为 BSON 整数时返回错误。
fn next_task_version(expected_task_version: u64) -> Result<i64> {
    let next = expected_task_version
        .checked_add(1)
        .ok_or(Error::EntityMetadataOutOfRange("version"))?;
    i64::try_from(next).map_err(|_| Error::EntityMetadataOutOfRange("version"))
}

fn approval_task_still_open(item: &WorkItem, execution_id: &ApprovalNodeExecutionId) -> bool {
    item.status == WorkItemStatus::Open && item.approval_node_execution_id.as_ref() == Some(execution_id)
}

fn insert_enum_filter<T: Copy>(
    filter: &mut Document,
    field: &str,
    values: &[T],
    code: impl Fn(T) -> &'static str,
) {
    match values {
        [] => {}
        [value] => {
            filter.insert(field, code(*value));
        }
        values => {
            filter.insert(
                field,
                doc! { "$in": values.iter().copied().map(code).collect::<Vec<_>>() },
            );
        }
    }
}

fn insert_string_filter(filter: &mut Document, field: &str, values: &[String]) {
    match values {
        [] => {}
        [value] => {
            filter.insert(field, value);
        }
        values => {
            filter.insert(field, doc! { "$in": values.to_vec() });
        }
    }
}

fn insert_due_range(filter: &mut Document, due_from: Option<Instant>, due_before: Option<Instant>) {
    let mut range = Document::new();
    if let Some(due_from) = due_from {
        range.insert("$gte", due_from.unix_secs());
    }
    if let Some(due_before) = due_before {
        range.insert("$lt", due_before.unix_secs());
    }
    if !range.is_empty() {
        filter.insert("due_at", range);
    }
}

fn literal_query_filter(query: &str) -> Document {
    let literal = regex::escape(query.trim());
    doc! {
        "$or": [
            { "business_object_id": { "$regex": &literal, "$options": "i" } },
            { "business_object_type": { "$regex": &literal, "$options": "i" } },
            { "responsibility_key": { "$regex": &literal, "$options": "i" } },
            { "reason_code": { "$regex": &literal, "$options": "i" } },
            { "impact_summary": { "$regex": &literal, "$options": "i" } },
        ]
    }
}

fn object_access_shape_filter(shapes: &[(WorkItemType, String)]) -> Document {
    if shapes.is_empty() {
        return doc! { "id": { "$exists": false } };
    }
    let alternatives = shapes
        .iter()
        .map(|(work_item_type, business_object_type)| {
            doc! {
                "work_item_type": work_item_type.as_str(),
                "business_object_type": business_object_type,
            }
        })
        .collect::<Vec<_>>();
    doc! { "$or": alternatives }
}

fn history_scope_filter(actor_id: Option<&str>, managed_organization_ids: Option<&[String]>) -> Document {
    let mut alternatives = Vec::new();
    if let Some(actor_id) = actor_id {
        alternatives.extend([
            doc! { "responsibility_actor_ids": actor_id },
            doc! { "completed_by": actor_id },
            doc! { "closed_by": actor_id },
        ]);
    }
    if let Some(organization_ids) = managed_organization_ids {
        if organization_ids.is_empty() {
            alternatives.push(Document::new());
        } else {
            alternatives.push(doc! { "owner_organization_id": { "$in": organization_ids.to_vec() } });
        }
    }
    doc! { "$or": alternatives }
}

fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        Some("due_at") => "due_at",
        Some("assigned_at") => "assigned_at",
        Some("current_assignment_at") => "current_assignment_at",
        Some("last_activity_at") => "last_activity_at",
        Some("completed_at") => "completed_at",
        Some("closed_at") => "closed_at",
        _ => "created_at",
    };
    doc! { field: direction }
}

fn work_item_projection() -> Document {
    doc! {
        "id": 1,
        "work_item_type": 1,
        "approval_node_execution_id": 1,
        "business_object_type": 1,
        "business_object_id": 1,
        "subject_version": 1,
        "status": 1,
        "owner_role": 1,
        "owner_organization_id": 1,
        "owner_user_id": 1,
        "responsibility_actor_ids": 1,
        "assignment_source": 1,
        "assigned_at": 1,
        "started_at": 1,
        "current_assignment_at": 1,
        "last_activity_at": 1,
        "priority": 1,
        "due_at": 1,
        "reason_code": 1,
        "impact_summary": 1,
        "completed_at": 1,
        "completed_by": 1,
        "closed_at": 1,
        "closed_by": 1,
        "close_reason": 1,
        "version": 1,
        "created_at": 1,
        "updated_at": 1,
    }
}

impl<'a, T> Repository<'a, T>
where
    T: Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    /// 按稳定 ID 批量读取工作项简报所需的同类业务对象。
    ///
    /// # 参数
    /// * `ids` - 业务对象稳定 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的业务对象；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_work_item_brief_entities_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }
}

impl<'a> Repository<'a, WorkItem> {
    /// 按稳定 ID 读取任意已注册工作项。
    ///
    /// # 参数
    /// * `id` - 工作项 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除工作项；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<WorkItem>> {
        self.find_by_id(id, executor).await
    }

    /// 查找指定业务对象当前开放的供应异常人工任务。
    ///
    /// # 参数
    /// * `business_object_type` - 业务对象类型
    /// * `business_object_id` - 业务对象稳定 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回开放的业务异常任务；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_open_business_exception_for_object(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        self.find_one(
            doc! {
                "work_item_type": WorkItemType::BusinessException.as_str(),
                "business_object_type": business_object_type,
                "business_object_id": business_object_id,
                "status": WorkItemStatus::Open.as_str(),
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, entities::AccountCore> {
    /// 按稳定 ID 读取工作项授权使用的账号事实。
    ///
    /// # 参数
    /// * `id` - 统一账号 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除账号；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::AccountCore>> {
        self.find_by_id(id, executor).await
    }

    /// 批量读取工作项负责人和提交人展示账号。
    ///
    /// # 参数
    /// * `ids` - 账号 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配且未删除的账号；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_work_item_party_accounts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<entities::AccountCore>> {
        self.list_work_item_brief_entities_by_ids(ids, executor).await
    }
}

impl<'a> Repository<'a, entities::AuditLog> {
    /// 按稳定 ID 读取工作项幂等命令审计。
    ///
    /// # 参数
    /// * `id` - 审计日志 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除审计；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_command_audit(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::AuditLog>> {
        self.find_by_id(id, executor).await
    }

    /// 批量读取指定资源的成功创建审计。
    ///
    /// # 参数
    /// * `resource_type` - 资源类型
    /// * `resource_ids` - 资源 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回动作固定为 `<resource_type>.create` 的成功审计。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_work_item_creation_audits(
        &self,
        resource_type: &str,
        resource_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<entities::AuditLog>> {
        if resource_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            doc! {
                "resource_type": resource_type,
                "resource_id": { "$in": resource_ids },
                "action": format!("{resource_type}.create"),
                "success": true,
            },
            executor,
        )
        .await
    }

    /// 批量读取指定资源的全部成功工作项事实审计。
    ///
    /// # 参数
    /// * `resource_type` - 资源类型
    /// * `resource_ids` - 资源 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回资源类型和 ID 命中的全部成功审计。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_successful_work_item_fact_audits(
        &self,
        resource_type: &str,
        resource_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<entities::AuditLog>> {
        if resource_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            doc! {
                "resource_type": resource_type,
                "resource_id": { "$in": resource_ids },
                "success": true,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, entities::purchase_order::PurchaseOrderSubmission> {
    /// 按稳定 ID 读取采购审核岗位分离使用的提交事实。
    ///
    /// # 参数
    /// * `id` - 采购提交 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除采购提交；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_purchase_submission(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::purchase_order::PurchaseOrderSubmission>> {
        self.find_by_id(id, executor).await
    }

    /// 批量读取采购单关联的全部简报提交。
    ///
    /// # 参数
    /// * `order_ids` - 采购单稳定 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回采购单命中的全部提交；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_work_item_brief_submissions_by_orders(
        &self,
        order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<entities::purchase_order::PurchaseOrderSubmission>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "purchase_order_id": { "$in": order_ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, entities::sales_order::SalesOrderSubmission> {
    /// 批量读取销售单关联的全部简报提交。
    ///
    /// # 参数
    /// * `order_ids` - 销售单稳定 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回销售单命中的全部提交；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_work_item_brief_submissions_by_orders(
        &self,
        order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<entities::sales_order::SalesOrderSubmission>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "sales_order_id": { "$in": order_ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, entities::sales_review::SalesChangeSubmission> {
    /// 按稳定 ID 读取销售变更岗位分离使用的提交事实。
    ///
    /// # 参数
    /// * `id` - 销售变更提交 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除销售变更提交；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_sales_change_submission(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::sales_review::SalesChangeSubmission>> {
        self.find_by_id(id, executor).await
    }
}

impl<'a> Repository<'a, entities::receivable::ReceivableAccount> {
    /// 按稳定 ID 读取票款复核任务使用的应收子账。
    ///
    /// # 参数
    /// * `id` - 应收子账 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除应收子账；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_receivable_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::receivable::ReceivableAccount>> {
        self.find_by_id(id, executor).await
    }
}

impl<'a> Repository<'a, entities::sales_order::SalesOrder> {
    /// 按稳定 ID 读取工作项当前销售单事实。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除销售单；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_sales_order(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::sales_order::SalesOrder>> {
        self.find_by_id(id, executor).await
    }
}

impl<'a> Repository<'a, entities::sales_order::SalesOrderRevision> {
    /// 按稳定 ID 读取工作项当前销售正式版本。
    ///
    /// # 参数
    /// * `id` - 销售正式版本 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除销售版本；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_sales_order_revision(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::sales_order::SalesOrderRevision>> {
        self.find_by_id(id, executor).await
    }
}

impl<'a> Repository<'a, entities::inventory::StockAdjustment> {
    /// 按稳定 ID 读取库存调整岗位分离事实。
    ///
    /// # 参数
    /// * `id` - 库存调整单 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除库存调整单；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_stock_adjustment(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::inventory::StockAdjustment>> {
        self.find_by_id(id, executor).await
    }
}

impl<'a> Repository<'a, entities::inventory::StockAdjustmentLine> {
    /// 批量读取库存调整简报明细。
    ///
    /// # 参数
    /// * `adjustment_ids` - 库存调整单 ID 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配的库存调整明细；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_work_item_brief_lines_by_adjustments(
        &self,
        adjustment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<entities::inventory::StockAdjustmentLine>> {
        if adjustment_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            doc! { "stock_adjustment_id": { "$in": adjustment_ids } },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, entities::supplier_settlement::SupplierSettlementStatement> {
    /// 按稳定 ID 读取供应商结算岗位分离事实。
    ///
    /// # 参数
    /// * `id` - 供应商结算单 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除结算单；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_supplier_settlement(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::supplier_settlement::SupplierSettlementStatement>> {
        self.find_by_id(id, executor).await
    }
}

impl<'a> Repository<'a, entities::integration_ops::IntegrationErrorTask> {
    /// 按稳定 ID 读取 W29 集成异常对象。
    ///
    /// # 参数
    /// * `id` - 集成异常任务 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除集成异常对象；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_integration_error_task(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::integration_ops::IntegrationErrorTask>> {
        self.find_by_id(id, executor).await
    }
}

impl<'a> Repository<'a, entities::integration_ops::ReconciliationDifference> {
    /// 按稳定 ID 读取 W29 对账差异对象。
    ///
    /// # 参数
    /// * `id` - 对账差异 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回未删除对账差异；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_work_item_reconciliation_difference(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::integration_ops::ReconciliationDifference>> {
        self.find_by_id(id, executor).await
    }
}

#[cfg(test)]
mod tests {
    use entity_core::HasBaseModel;
    use mongodb::bson::{doc, Bson};

    use super::{
        approval_task_still_open, open_approval_execution_filter, sort_doc, work_item_projection,
        QueryFilter, WorkItemFilter, WorkItemRow,
    };
    use crate::repository::bpm::{approval_task_cas_filter, classify_cas_miss, CasWriteOutcome};
    use bpm::ApprovalNodeExecutionId;
    use entities::common::time::Instant;
    use entities::ids::WorkItemId;
    use entities::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
    };

    fn assigned_item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::ImportBusinessConfirmation,
                business_object_type: "LEGACY_IMPORT_BATCH".to_string(),
                business_object_id: "batch-1".to_string(),
                subject_version: "v1".to_string(),
                owner_role: "sales".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "alice".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap()
    }

    #[test]
    fn scope_filter_supports_direct_owner_and_history_facts() {
        let mine = WorkItemFilter {
            statuses: vec![WorkItemStatus::Open],
            work_item_types: vec![
                WorkItemType::ImportBusinessConfirmation,
                WorkItemType::BusinessException,
            ],
            owner_user_id: Some("alice".to_string()),
            owner_organization_ids: vec!["org-1".to_string()],
            priorities: vec![WorkItemPriority::High, WorkItemPriority::Urgent],
            due_from: Some(Instant::from_unix_secs(100)),
            due_before: Some(Instant::from_unix_secs(200)),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        assert_eq!(mine.get_str("status").unwrap(), "OPEN");
        assert_eq!(mine.get_str("owner_user_id").unwrap(), "alice");
        assert_eq!(
            mine.get_document("due_at").unwrap(),
            &doc! { "$gte": 100_i64, "$lt": 200_i64 }
        );
        assert_eq!(
            mine.get_document("priority").unwrap(),
            &doc! { "$in": ["high", "urgent"] }
        );
        assert_eq!(
            mine.get_document("work_item_type").unwrap(),
            &doc! { "$in": ["IMPORT_BUSINESS_CONFIRMATION", "BUSINESS_EXCEPTION"] }
        );

        let history = WorkItemFilter {
            statuses: vec![WorkItemStatus::Completed, WorkItemStatus::Closed],
            history_actor_id: Some("alice".to_string()),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        let history_or = history.get_array("$and").unwrap()[0]
            .as_document()
            .unwrap()
            .get_array("$or")
            .unwrap();
        assert_eq!(
            history_or,
            &vec![
                Bson::Document(doc! { "responsibility_actor_ids": "alice" }),
                Bson::Document(doc! { "completed_by": "alice" }),
                Bson::Document(doc! { "closed_by": "alice" }),
            ]
        );
    }

    #[test]
    fn text_query_is_literal_and_composes_with_history_scope() {
        let filter = WorkItemFilter {
            statuses: vec![WorkItemStatus::Completed, WorkItemStatus::Closed],
            history_actor_id: Some("alice".to_string()),
            query: Some("SO.[1]".to_string()),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();

        let conjunctions = filter.get_array("$and").unwrap();
        assert_eq!(conjunctions.len(), 2);
        let query = conjunctions[1].as_document().unwrap().get_array("$or").unwrap();
        let regex = query[0]
            .as_document()
            .unwrap()
            .get_document("business_object_id")
            .unwrap()
            .get_str("$regex")
            .unwrap();
        assert_eq!(regex, r"SO\.\[1\]");
    }

    #[test]
    fn history_scope_unions_actor_and_managed_organizations() {
        let filter = WorkItemFilter {
            history_actor_id: Some("alice".to_string()),
            history_managed_organization_ids: Some(vec!["org-a".to_string()]),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();

        let history = filter.get_array("$and").unwrap()[0]
            .as_document()
            .unwrap()
            .get_array("$or")
            .unwrap();
        assert_eq!(history.len(), 4);
        assert_eq!(
            history[3],
            Bson::Document(doc! { "owner_organization_id": { "$in": ["org-a"] } })
        );
    }

    #[test]
    fn object_access_shapes_fail_closed_and_pair_type_with_object() {
        let denied = WorkItemFilter {
            object_access_shapes: Some(Vec::new()),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        assert_eq!(
            denied.get_array("$and").unwrap()[0],
            Bson::Document(doc! { "id": { "$exists": false } })
        );

        let allowed = WorkItemFilter {
            object_access_shapes: Some(vec![(
                WorkItemType::PurchaseOrderReview,
                "purchase_order".to_string(),
            )]),
            page: 1,
            page_size: 20,
            ..WorkItemFilter::default()
        }
        .to_doc();
        assert_eq!(
            allowed.get_array("$and").unwrap()[0],
            Bson::Document(doc! { "$or": [{
                "work_item_type": "PURCHASE_ORDER_REVIEW",
                "business_object_type": "purchase_order",
            }] })
        );
    }

    #[test]
    fn sort_doc_is_whitelisted() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("last_activity_at"), true),
            doc! { "last_activity_at": 1 }
        );
        assert_eq!(sort_doc(Some("assigned_at"), false), doc! { "assigned_at": -1 });
        assert_eq!(
            sort_doc(Some("business_object_id"), false),
            doc! { "created_at": -1 }
        );
    }

    /// 节点执行开放任务查询固定约束审批类型、开放状态和执行引用。
    ///
    /// 该形态命中执行唯一索引且不会混入独立任务。
    #[test]
    fn open_approval_execution_filter_is_semantic_and_bounded() {
        let filter = open_approval_execution_filter(&ApprovalNodeExecutionId::new("exec-1"));
        assert_eq!(filter.len(), 3);
        assert_eq!(filter.get_str("work_item_type").unwrap(), "DOCUMENT_APPROVAL");
        assert_eq!(filter.get_str("status").unwrap(), "OPEN");
        assert_eq!(filter.get_str("approval_node_execution_id").unwrap(), "exec-1");
    }

    #[test]
    fn approval_task_cas_miss_classifies_closed_and_version() {
        let execution = ApprovalNodeExecutionId::new("exec-1");
        let filter = approval_task_cas_filter("wi-1", 3, &execution).unwrap();
        assert_eq!(filter.get_str("status").unwrap(), "OPEN");
        assert_eq!(filter.get_str("approval_node_execution_id").unwrap(), "exec-1");

        let mut closed = assigned_item();
        closed.status = WorkItemStatus::Closed;
        closed.approval_node_execution_id = Some(execution.clone());
        assert!(!approval_task_still_open(&closed, &execution));
        let closed_version = closed.base().version;
        assert!(matches!(
            classify_cas_miss(Some(closed), closed_version, |item| {
                approval_task_still_open(item, &execution)
            }),
            CasWriteOutcome::StatusChanged(_)
        ));

        let mut stale = assigned_item();
        stale.approval_node_execution_id = Some(execution.clone());
        stale.base_mut().version = 4;
        assert!(matches!(
            classify_cas_miss(Some(stale), 3, |item| approval_task_still_open(item, &execution)),
            CasWriteOutcome::VersionConflict(_)
        ));

        let mut open_wrong = assigned_item();
        open_wrong.approval_node_execution_id = Some(ApprovalNodeExecutionId::new("exec-2"));
        let open_version = open_wrong.base().version;
        assert!(!approval_task_still_open(&open_wrong, &execution));
        assert!(matches!(
            classify_cas_miss(Some(open_wrong), open_version, |item| {
                approval_task_still_open(item, &execution)
            }),
            CasWriteOutcome::StatusChanged(_)
        ));
        assert!(matches!(
            classify_cas_miss::<WorkItem>(None, 1, |item| approval_task_still_open(item, &execution)),
            CasWriteOutcome::NotFound
        ));
    }

    #[test]
    fn work_item_projection_includes_approval_node_execution_id() {
        let projection = work_item_projection();
        assert_eq!(projection.get_i32("approval_node_execution_id").unwrap(), 1);
        let row = WorkItemRow {
            id: "wi-1".to_string(),
            work_item_type: WorkItemType::CardFundsDeltaReview,
            approval_node_execution_id: Some("exec-1".to_string()),
            business_object_type: "receivable_account".to_string(),
            business_object_id: "account-1".to_string(),
            subject_version: "v1".to_string(),
            status: WorkItemStatus::Open,
            owner_role: "role-finance".to_string(),
            owner_organization_id: "org-1".to_string(),
            owner_user_id: Some("user-1".to_string()),
            responsibility_actor_ids: Vec::new(),
            assignment_source: AssignmentSource::SystemRule,
            assigned_at: None,
            started_at: None,
            current_assignment_at: None,
            last_activity_at: None,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: None,
            impact_summary: None,
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
            version: 1,
            created_at: 1,
            updated_at: 1,
        };
        assert_eq!(row.approval_node_execution_id.as_deref(), Some("exec-1"));
        let decoded: WorkItemRow = mongodb::bson::deserialize_from_document(doc! {
            "id": "wi-2",
            "work_item_type": "CARD_FUNDS_DELTA_REVIEW",
            "business_object_type": "receivable_account",
            "business_object_id": "account-1",
            "subject_version": "v1",
            "status": "OPEN",
            "owner_role": "role-finance",
            "owner_organization_id": "org-1",
            "owner_user_id": "user-1",
            "responsibility_actor_ids": [],
            "assignment_source": "SYSTEM_RULE",
            "priority": "normal",
            "version": 1i64,
            "created_at": 1i64,
            "updated_at": 1i64,
        })
        .expect("缺少 approval_node_execution_id 的旧文档仍可反序列化");
        assert_eq!(decoded.approval_node_execution_id, None);
    }
}
