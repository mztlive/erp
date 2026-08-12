use entities::common::time::Instant;
use entities::ids::MallSalesOrderSnapshotId;
use entities::mall_sync::{MappingTaskStatus, MappingTaskType, MasterMappingTask};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 映射任务列表允许的排序字段白名单。
pub(crate) const MASTER_MAPPING_TASK_SORT_FIELDS: &[&str] = &["created_at", "resolved_at"];

/// 映射任务创建请求（同一快照、映射类型只允许一个进行中任务）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateMasterMappingTaskRequest {
    /// 待处理快照。
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
    /// 业务责任角色。
    #[validate(custom(function = "non_blank", message = "责任角色不能为空"))]
    pub owner_role: String,
    /// 业务责任用户 ID（可按角色领办，可为空）。
    pub owner_user_id: Option<String>,
}

/// 映射任务响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MasterMappingTaskView {
    /// 实体主键。
    pub id: String,
    /// 待处理快照。
    pub source_snapshot_id: String,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
    /// 任务状态。
    pub status: MappingTaskStatus,
    /// 业务责任角色。
    pub owner_role: String,
    /// 业务责任用户 ID。
    pub owner_user_id: Option<String>,
    /// 处理结论。
    pub resolution: Option<String>,
    /// 完成时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MasterMappingTask> for MasterMappingTaskView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `task` - 映射任务实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(task: MasterMappingTask) -> Self {
        Self {
            id: task.base.id,
            source_snapshot_id: task.source_snapshot_id.to_string(),
            mapping_type: task.mapping_type,
            status: task.status,
            owner_role: task.owner_role,
            owner_user_id: task.owner_user_id,
            resolution: task.resolution,
            resolved_at: task.resolved_at,
            version: task.base.version,
            created_at: task.base.created_at,
        }
    }
}

/// 映射任务列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MasterMappingTaskListParams {
    /// 待处理快照筛选。
    pub source_snapshot_id: Option<MallSalesOrderSnapshotId>,
    /// 映射类型筛选。
    pub mapping_type: Option<MappingTaskType>,
    /// 任务状态筛选。
    pub status: Option<MappingTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 责任用户 ID 筛选。
    pub owner_user_id: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`resolved_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的映射任务列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MasterMappingTaskListQuery {
    /// 待处理快照筛选。
    pub source_snapshot_id: Option<MallSalesOrderSnapshotId>,
    /// 映射类型筛选。
    pub mapping_type: Option<MappingTaskType>,
    /// 任务状态筛选。
    pub status: Option<MappingTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 责任用户 ID 筛选。
    pub owner_user_id: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MasterMappingTaskListParams {
    /// 归一化映射任务列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MasterMappingTaskListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, MASTER_MAPPING_TASK_SORT_FIELDS)?;
        Ok(MasterMappingTaskListQuery {
            source_snapshot_id: self.source_snapshot_id.clone(),
            mapping_type: self.mapping_type,
            status: self.status,
            owner_role: normalized_text(self.owner_role.as_deref()),
            owner_user_id: normalized_text(self.owner_user_id.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 映射任务处理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveTaskKind {
    /// 已解决。
    Resolved,
    /// 无法处理。
    Unresolvable,
}

/// 映射任务处理请求（处理结论必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResolveMasterMappingTaskRequest {
    /// 处理方式。
    pub kind: ResolveTaskKind,
    /// 处理结论（映射结果说明或无法处理原因）。
    #[validate(custom(function = "non_blank", message = "处理结论不能为空"))]
    pub resolution: String,
}

#[cfg(test)]
mod tests {
    use super::MasterMappingTaskListParams;
    use crate::mall_sync::dto::common::SortDir;

    #[test]
    fn mapping_query_normalizes_text_paging_and_sort_defaults() {
        let query = MasterMappingTaskListParams {
            source_snapshot_id: None,
            mapping_type: None,
            status: None,
            owner_role: Some(" owner ".to_string()),
            owner_user_id: Some("   ".to_string()),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        }
        .normalized()
        .unwrap();
        assert_eq!(query.owner_role.as_deref(), Some("owner"));
        assert_eq!(query.owner_user_id, None);
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }
}
