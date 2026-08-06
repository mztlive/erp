//! 域 D03 `work_item` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。

use entities::work_item::{
    WorkItem, WorkItemCloseData, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 待办列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const WORK_ITEM_SORT_FIELDS: &[&str] = &["created_at", "updated_at", "due_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空对象类型/ID 需要按「空白视为空」拒绝，落入 HTTP 400）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 待办响应视图（契约形状：W01/W02 队列投影所需字段 + 乐观锁版本）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemView {
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
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
    /// 该任务唯一允许的完成动作。
    pub completion_action: String,
    /// 正式完成审计时间（秒级时间戳）。
    pub completed_at: Option<u64>,
    /// 正式完成执行人。
    pub completed_by: Option<String>,
    /// 关闭原因代码。
    pub close_reason_code: Option<String>,
    /// 关闭原因说明。
    pub close_reason_text: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<WorkItem> for WorkItemView {
    /// 从实体构造响应视图。
    fn from(item: WorkItem) -> Self {
        Self {
            id: item.base.id,
            work_item_type: item.work_item_type,
            business_object_type: item.business_object_type,
            business_object_id: item.business_object_id,
            subject_version: item.subject_version,
            status: item.status,
            owner_role: item.owner_role,
            owner_user_id: item.owner_user_id,
            priority: item.priority,
            due_at: item.due_at.map(|instant| instant.unix_secs() as u64),
            reason_code: item.reason_code,
            impact_summary: item.impact_summary,
            completion_action: item.completion_action,
            completed_at: item.completed_at.map(|instant| instant.unix_secs() as u64),
            completed_by: item.completed_by,
            close_reason_code: item.close_reason_code,
            close_reason_text: item.close_reason_text,
            version: item.base.version,
            created_at: item.base.created_at,
        }
    }
}

/// 待办列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WorkItemListParams {
    /// 任务类型筛选。
    pub work_item_type: Option<WorkItemType>,
    /// 任务状态筛选。
    pub status: Option<WorkItemStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 当前责任人筛选。
    pub owner_user_id: Option<String>,
    /// 优先级筛选。
    pub priority: Option<WorkItemPriority>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`/`due_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的待办列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkItemListQuery {
    /// 任务类型筛选。
    pub work_item_type: Option<WorkItemType>,
    /// 任务状态筛选。
    pub status: Option<WorkItemStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 当前责任人筛选。
    pub owner_user_id: Option<String>,
    /// 优先级筛选。
    pub priority: Option<WorkItemPriority>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl WorkItemListParams {
    /// 归一化待办列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<WorkItemListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, WORK_ITEM_SORT_FIELDS)?;
        Ok(WorkItemListQuery {
            work_item_type: self.work_item_type,
            status: self.status,
            owner_role: normalized_text(self.owner_role.as_deref()),
            owner_user_id: normalized_text(self.owner_user_id.as_deref()),
            priority: self.priority,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 派发待办请求（HTTP 契约：`{ work_item_type, business_object_type,
/// business_object_id, ... }`；责任人创建时不领取）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DispatchWorkItemRequest {
    /// 任务类型（固定枚举，禁止临时创造同义代码）。
    pub work_item_type: WorkItemType,
    /// 业务对象类型代码（跨域开放目录）。
    #[validate(custom(function = "non_blank", message = "业务对象类型不能为空"))]
    pub business_object_type: String,
    /// 业务对象 ID。
    #[validate(custom(function = "non_blank", message = "业务对象ID不能为空"))]
    pub business_object_id: String,
    /// 任务针对的对象版本（适用时）。
    #[validate(length(max = 64, message = "对象版本过长"))]
    pub subject_version: Option<String>,
    /// 责任角色。
    #[validate(length(max = 128, message = "责任角色过长"))]
    pub owner_role: Option<String>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限（秒级时间戳）。
    #[validate(range(min = 1, message = "时限必须大于 0"))]
    pub due_at: Option<u64>,
    /// 产生原因代码。
    #[validate(length(max = 64, message = "原因代码过长"))]
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    #[validate(length(max = 512, message = "影响摘要过长"))]
    pub impact_summary: Option<String>,
    /// 该任务唯一允许的完成动作。
    #[validate(custom(function = "non_blank", message = "完成动作不能为空"))]
    pub completion_action: String,
}

impl DispatchWorkItemRequest {
    /// 转换为实体创建数据。
    ///
    /// # 返回
    /// 返回实体层创建数据（初始 `UNCLAIMED`）。
    pub(crate) fn into_data(self) -> WorkItemData {
        WorkItemData {
            work_item_type: self.work_item_type,
            business_object_type: self.business_object_type,
            business_object_id: self.business_object_id,
            subject_version: self.subject_version,
            owner_role: self.owner_role,
            owner_user_id: None,
            priority: self.priority,
            due_at: self
                .due_at
                .map(|secs| entities::common::time::Instant::from_unix_secs(secs as i64)),
            reason_code: self.reason_code,
            impact_summary: self.impact_summary,
            completion_action: self.completion_action,
        }
    }
}

/// 领取待办请求（携带期望版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ClaimWorkItemRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 暂挂待办请求（携带期望版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DeferWorkItemRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 暂挂原因说明。
    #[validate(length(max = 512, message = "暂挂原因过长"))]
    pub comment: Option<String>,
}

/// 转交待办请求（携带期望版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TransferWorkItemRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 新责任角色。
    #[validate(custom(function = "non_blank", message = "责任角色不能为空"))]
    pub owner_role: String,
    /// 新责任人。
    #[validate(custom(function = "non_blank", message = "责任人不能为空"))]
    pub owner_user_id: String,
    /// 转交原因。
    #[validate(length(max = 512, message = "转交原因过长"))]
    pub comment: Option<String>,
}

/// 完成待办请求（携带期望版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompleteWorkItemRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 关闭待办请求（携带期望版本，冲突返回 409；关闭必须记录结构化原因）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CloseWorkItemRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 关闭原因代码（结构化原因，必填）。
    #[validate(custom(function = "non_blank", message = "关闭原因代码不能为空"))]
    pub close_reason_code: String,
    /// 关闭原因说明。
    #[validate(length(max = 512, message = "关闭原因过长"))]
    pub close_reason_text: Option<String>,
}

impl CloseWorkItemRequest {
    /// 转换为实体关闭数据。
    ///
    /// # 返回
    /// 返回实体层关闭数据。
    pub(crate) fn into_close_data(self) -> WorkItemCloseData {
        WorkItemCloseData {
            close_reason_code: self.close_reason_code,
            close_reason_text: self.close_reason_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, DispatchWorkItemRequest, SortDir, TransferWorkItemRequest, WorkItemListParams,
    };
    use entities::work_item::{WorkItemPriority, WorkItemStatus, WorkItemType};
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("owner_user_id".to_string()), &None, &["due_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["due_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" due_at ".to_string()),
            &Some(" asc ".to_string()),
            &["due_at"],
        )
        .unwrap();
        assert_eq!(field, "due_at");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = WorkItemListParams {
            work_item_type: Some(WorkItemType::ImportBusinessConfirmation),
            status: Some(WorkItemStatus::InProgress),
            owner_role: Some(" sales ".to_string()),
            owner_user_id: Some(" user-1 ".to_string()),
            priority: Some(WorkItemPriority::High),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(
            query.work_item_type,
            Some(WorkItemType::ImportBusinessConfirmation)
        );
        assert_eq!(query.status, Some(WorkItemStatus::InProgress));
        assert_eq!(query.owner_role.as_deref(), Some("sales"));
        assert_eq!(query.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn dispatch_request_converts_to_entity_data() {
        let request: DispatchWorkItemRequest = serde_json::from_value(json!({
            "work_item_type": "IMPORT_BUSINESS_CONFIRMATION",
            "business_object_type": "LEGACY_IMPORT_BATCH",
            "business_object_id": "batch-1",
            "owner_role": "sales",
            "priority": "normal",
            "due_at": 1700604800,
            "completion_action": "COMPLETE_IMPORT_BUSINESS_CONFIRMATION",
        }))
        .unwrap();
        let data = request.into_data();
        assert_eq!(data.business_object_type, "LEGACY_IMPORT_BATCH");
        assert!(data.owner_user_id.is_none());
        assert_eq!(data.completion_action, "COMPLETE_IMPORT_BUSINESS_CONFIRMATION");
    }

    #[test]
    fn stale_version_and_blank_role_are_rejected() {
        let request = TransferWorkItemRequest {
            version: 0,
            owner_role: "  ".to_string(),
            owner_user_id: "user-1".to_string(),
            comment: None,
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = WorkItemListParams {
            work_item_type: None,
            status: None,
            owner_role: None,
            owner_user_id: None,
            priority: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }
}
