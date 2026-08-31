//! 域 D06 `access_control` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。
//!
//! 角色/账号/Casbin 能力已有 `services::iam` 承载，本域 DTO 只覆盖增补的
//! `permission` 目录、`data_scope`、`user_role` 绑定记录与 `audit_event` 查询
//! （domains.md：D06 只做 data_scope 增补与 audit_log→audit_event 字段对齐）。

use entities::access_control::{
    AuditEvent, AuditEventResult, DataScope, DataScopeData, DataScopeSubjectType, DataScopeType, Permission,
    PermissionData, PermissionUpdate, UserRole, UserRoleData, UserRoleRevokeData,
};
use entities::rbac::RoleId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 权限定义列表允许的排序字段白名单。
pub(crate) const PERMISSION_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];
/// 数据范围列表允许的排序字段白名单。
pub(crate) const DATA_SCOPE_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];
/// 审计事件列表允许的排序字段白名单。
pub(crate) const AUDIT_EVENT_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

/// 排序方向。
pub use crate::query::SortDir;

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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空。
use crate::query::non_blank;

/// 权限定义响应视图（W19 权限目录）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PermissionView {
    /// 实体主键。
    pub id: String,
    /// 权限资源。
    pub resource: String,
    /// 权限动作。
    pub action: String,
    /// 展示名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 系统内建权限标记。
    pub system: bool,
    /// 停用标记。
    pub disabled: bool,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<Permission> for PermissionView {
    /// 从实体构造响应视图。
    fn from(permission: Permission) -> Self {
        Self {
            id: permission.base.id,
            resource: permission.resource,
            action: permission.action,
            name: permission.name,
            description: permission.description,
            system: permission.system,
            disabled: permission.disabled,
            version: permission.base.version,
            created_at: permission.base.created_at,
        }
    }
}

/// 权限定义创建请求（`resource:action` 经实体复用 `rbac::Permission::parse` 规范化）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePermissionRequest {
    /// 权限资源（如 `sales_order`）。
    #[validate(custom(function = "non_blank", message = "权限资源不能为空"))]
    pub resource: String,
    /// 权限动作（如 `approve`）。
    #[validate(custom(function = "non_blank", message = "权限动作不能为空"))]
    pub action: String,
    /// 展示名称。
    #[validate(custom(function = "non_blank", message = "权限名称不能为空"))]
    pub name: String,
    /// 描述。
    #[validate(length(max = 256, message = "权限描述过长"))]
    pub description: Option<String>,
    /// 是否为系统内建权限（禁止删除/修改）；缺省视为自定义权限。
    #[serde(default)]
    pub system: bool,
}

impl CreatePermissionRequest {
    /// 转换为实体创建数据。
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub(crate) fn into_data(self) -> PermissionData {
        PermissionData {
            resource: self.resource,
            action: self.action,
            name: self.name,
            description: self.description,
            system: self.system,
        }
    }
}

/// 权限定义更新请求（携带乐观锁版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePermissionRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 展示名称；缺省表示不修改。
    pub name: Option<String>,
    /// 描述；缺省表示不修改。
    pub description: Option<String>,
    /// 停用标记；缺省表示不修改。
    pub disabled: Option<bool>,
}

impl UpdatePermissionRequest {
    /// 返回本次补丁显式携带的权限字段名。
    ///
    /// # 返回
    /// 按 `name`、`description`、`disabled` 的稳定合同顺序返回字段名；
    /// 未携带任何可更新字段时返回空集合。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn changed_field_names(&self) -> Vec<String> {
        let mut changed = Vec::new();
        if self.name.is_some() {
            changed.push("name".to_string());
        }
        if self.description.is_some() {
            changed.push("description".to_string());
        }
        if self.disabled.is_some() {
            changed.push("disabled".to_string());
        }
        changed
    }

    /// 转换为实体更新数据。
    ///
    /// # 返回
    /// 返回实体层更新数据。
    pub(crate) fn into_update(self) -> PermissionUpdate {
        PermissionUpdate {
            name: self.name,
            description: self.description,
            disabled: self.disabled,
        }
    }
}

/// 权限定义列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PermissionListParams {
    /// 权限资源模糊筛选（忽略大小写）。
    pub resource: Option<String>,
    /// 停用标记筛选。
    pub disabled: Option<bool>,
    /// 是否仅系统内建筛选。
    pub system: Option<bool>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的权限定义列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PermissionListQuery {
    /// 权限资源模糊筛选。
    pub resource: Option<String>,
    /// 停用标记筛选。
    pub disabled: Option<bool>,
    /// 是否仅系统内建筛选。
    pub system: Option<bool>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl PermissionListParams {
    /// 归一化权限定义列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<PermissionListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, PERMISSION_SORT_FIELDS)?;
        Ok(PermissionListQuery {
            resource: normalized_text(self.resource.as_deref()),
            disabled: self.disabled,
            system: self.system,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 数据范围响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DataScopeView {
    /// 实体主键。
    pub id: String,
    /// 范围主体类型。
    pub subject_type: DataScopeSubjectType,
    /// 范围主体 ID（角色 ID 或用户 ID）。
    pub subject_id: String,
    /// 范围类型。
    pub scope_type: DataScopeType,
    /// 范围对象（组织/团队 ID；公司、本人负责、协作参与不携带目标）。
    pub scope_targets: Vec<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<DataScope> for DataScopeView {
    /// 从实体构造响应视图。
    fn from(scope: DataScope) -> Self {
        Self {
            id: scope.base.id,
            subject_type: scope.subject_type,
            subject_id: scope.subject_id,
            scope_type: scope.scope_type,
            scope_targets: scope.scope_targets,
            version: scope.base.version,
            created_at: scope.base.created_at,
        }
    }
}

/// 数据范围创建请求（主体 + 范围类型唯一）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateDataScopeRequest {
    /// 范围主体类型。
    pub subject_type: DataScopeSubjectType,
    /// 范围主体 ID（角色 ID 或用户 ID）。
    #[validate(custom(function = "non_blank", message = "范围主体ID不能为空"))]
    pub subject_id: String,
    /// 范围类型。
    pub scope_type: DataScopeType,
    /// 范围对象（组织/团队 ID；公司、本人负责、协作参与不携带目标）。
    #[validate(length(max = 128, message = "范围目标数量不能超过128"))]
    pub scope_targets: Vec<String>,
}

impl CreateDataScopeRequest {
    /// 转换为实体创建数据。
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub(crate) fn into_data(self) -> DataScopeData {
        DataScopeData {
            subject_type: self.subject_type,
            subject_id: self.subject_id,
            scope_type: self.scope_type,
            scope_targets: self.scope_targets,
        }
    }
}

/// 数据范围列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DataScopeListParams {
    /// 范围主体类型筛选。
    pub subject_type: Option<DataScopeSubjectType>,
    /// 范围类型筛选。
    pub scope_type: Option<DataScopeType>,
    /// 范围主体 ID 筛选（与 `subject_type` 成对使用，走按主体查询）。
    pub subject_id: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的数据范围列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataScopeListQuery {
    /// 范围主体类型筛选。
    pub subject_type: Option<DataScopeSubjectType>,
    /// 范围类型筛选。
    pub scope_type: Option<DataScopeType>,
    /// 范围主体 ID 筛选。
    pub subject_id: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl DataScopeListParams {
    /// 归一化数据范围列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验；按主体查询
    /// 时必须同时提供 `subject_type`。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单、方向非法，或按主体查询缺少 `subject_type` 时返回
    /// `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<DataScopeListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, DATA_SCOPE_SORT_FIELDS)?;
        let subject_id = normalized_text(self.subject_id.as_deref());
        if subject_id.is_some() && self.subject_type.is_none() {
            return Err(Error::ValidationError(
                "按主体查询时必须提供范围主体类型".to_string(),
            ));
        }
        Ok(DataScopeListQuery {
            subject_type: self.subject_type,
            scope_type: self.scope_type,
            subject_id,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 用户角色绑定响应视图（W19 用户授权；含撤权历史字段，只读展示）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UserRoleView {
    /// 实体主键。
    pub id: String,
    /// 用户 ID。
    pub user_id: String,
    /// 角色。
    pub role_id: String,
    /// 生效时间（秒级时间戳）。
    pub effective_from: u64,
    /// 到期时间（秒级时间戳）。
    pub effective_to: Option<u64>,
    /// 分配人。
    pub assigned_by: String,
    /// 撤权时间（秒级时间戳）。
    pub revoked_at: Option<u64>,
    /// 撤权执行人。
    pub revoked_by: Option<String>,
    /// 撤权原因代码。
    pub revoke_reason_code: Option<String>,
    /// 撤权原因说明。
    pub revoke_reason_text: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<UserRole> for UserRoleView {
    /// 从实体构造响应视图。
    fn from(binding: UserRole) -> Self {
        Self {
            id: binding.base.id,
            user_id: binding.user_id,
            role_id: binding.role_id.to_string(),
            effective_from: binding.effective_from.unix_secs() as u64,
            effective_to: binding.effective_to.map(|instant| instant.unix_secs() as u64),
            assigned_by: binding.assigned_by,
            revoked_at: binding.revoked_at.map(|instant| instant.unix_secs() as u64),
            revoked_by: binding.revoked_by,
            revoke_reason_code: binding.revoke_reason_code,
            revoke_reason_text: binding.revoke_reason_text,
            version: binding.base.version,
            created_at: binding.base.created_at,
        }
    }
}

/// 用户角色绑定列表查询参数（按用户展示，`user_id` 必填）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UserRoleListParams {
    /// 用户 ID。
    #[validate(custom(function = "non_blank", message = "用户ID不能为空"))]
    pub user_id: String,
}

/// 分配用户角色请求（`effective_from` 缺省为当前时刻；分配人由服务端注入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AssignUserRoleRequest {
    /// 用户 ID。
    #[validate(custom(function = "non_blank", message = "用户ID不能为空"))]
    pub user_id: String,
    /// 角色（`RoleId` 解析校验）。
    pub role_id: RoleId,
    /// 生效时间（秒级时间戳）；缺省为当前时刻。
    #[validate(range(min = 1, message = "生效时间必须大于 0"))]
    pub effective_from: Option<u64>,
    /// 到期时间（秒级时间戳）；必须晚于生效时间。
    #[validate(range(min = 1, message = "到期时间必须大于 0"))]
    pub effective_to: Option<u64>,
}

impl AssignUserRoleRequest {
    /// 转换为实体创建数据。
    ///
    /// # 参数
    /// * `assigned_by` - 分配人（账号或系统身份）
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub(crate) fn into_data(self, assigned_by: &str) -> UserRoleData {
        UserRoleData {
            user_id: self.user_id,
            role_id: self.role_id,
            effective_from: entities::common::time::Instant::from_unix_secs(
                self.effective_from
                    .map(|secs| secs as i64)
                    .unwrap_or_else(|| entities::common::time::Instant::now().unix_secs()),
            ),
            effective_to: self
                .effective_to
                .map(|secs| entities::common::time::Instant::from_unix_secs(secs as i64)),
            assigned_by: assigned_by.to_string(),
        }
    }
}

/// 撤权命令（当前绑定版本由后端在事务内读取；撤权必须记录结构化原因）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RevokeUserRoleRequest {
    /// 撤权原因代码（结构化，必填）。
    #[validate(custom(function = "non_blank", message = "撤权原因代码不能为空"))]
    pub revoke_reason_code: String,
    /// 撤权原因说明。
    #[validate(length(max = 64, message = "撤权原因过长"))]
    pub revoke_reason_text: Option<String>,
}

impl RevokeUserRoleRequest {
    /// 转换为实体撤权数据。
    ///
    /// # 返回
    /// 返回实体层撤权数据。
    pub(crate) fn into_revoke_data(self) -> UserRoleRevokeData {
        UserRoleRevokeData {
            revoke_reason_code: self.revoke_reason_code,
            revoke_reason_text: self.revoke_reason_text,
        }
    }
}

/// 审计事件响应视图（W19 §5.2；敏感字段只返回「已变更」与摘要）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuditEventView {
    /// 实体主键。
    pub id: String,
    /// 操作者 ID。
    pub actor_id: String,
    /// 操作者名称快照。
    pub actor_label: String,
    /// 责任角色快照。
    pub actor_role: String,
    /// 动作代码。
    pub action_type: String,
    /// 业务对象类型代码。
    pub object_type: String,
    /// 业务对象 ID。
    pub object_id: Option<String>,
    /// 业务对象安全标题。
    pub object_label: Option<String>,
    /// 请求追踪号。
    pub request_id: Option<String>,
    /// 链路追踪号。
    pub trace_id: Option<String>,
    /// 最终结果。
    pub result: AuditEventResult,
    /// 变更字段名（只记录字段名和「已变更」）。
    pub changed_field_names: Vec<String>,
    /// 安全摘要。
    pub safe_digest: Option<String>,
    /// 来源 IP。
    pub source_ip: Option<String>,
    /// 设备上下文。
    pub device_context: Option<String>,
    /// 创建时间（秒级时间戳，即事件发生时间）。
    pub created_at: u64,
}

impl From<AuditEvent> for AuditEventView {
    /// 从实体构造响应视图。
    fn from(event: AuditEvent) -> Self {
        Self {
            id: event.base.id,
            actor_id: event.actor_id,
            actor_label: event.actor_label,
            actor_role: event.actor_role,
            action_type: event.action_type,
            object_type: event.object_type,
            object_id: event.object_id,
            object_label: event.object_label,
            request_id: event.request_id,
            trace_id: event.trace_id,
            result: event.result,
            changed_field_names: event.changed_field_names,
            safe_digest: event.safe_digest,
            source_ip: event.source_ip,
            device_context: event.device_context,
            created_at: event.base.created_at,
        }
    }
}

/// 审计事件列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AuditEventListParams {
    /// 操作者 ID 模糊筛选（忽略大小写）。
    pub actor_id: Option<String>,
    /// 动作代码模糊筛选（忽略大小写）。
    pub action_type: Option<String>,
    /// 业务对象类型代码筛选。
    pub object_type: Option<String>,
    /// 业务对象 ID 筛选。
    pub object_id: Option<String>,
    /// 最终结果筛选。
    pub result: Option<AuditEventResult>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的审计事件列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditEventListQuery {
    /// 操作者 ID 模糊筛选。
    pub actor_id: Option<String>,
    /// 动作代码模糊筛选。
    pub action_type: Option<String>,
    /// 业务对象类型代码筛选。
    pub object_type: Option<String>,
    /// 业务对象 ID 筛选。
    pub object_id: Option<String>,
    /// 最终结果筛选。
    pub result: Option<AuditEventResult>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl AuditEventListParams {
    /// 归一化审计事件列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<AuditEventListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, AUDIT_EVENT_SORT_FIELDS)?;
        Ok(AuditEventListQuery {
            actor_id: normalized_text(self.actor_id.as_deref()),
            action_type: normalized_text(self.action_type.as_deref()),
            object_type: normalized_text(self.object_type.as_deref()),
            object_id: normalized_text(self.object_id.as_deref()),
            result: self.result,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, AssignUserRoleRequest, AuditEventListParams, CreateDataScopeRequest,
        CreatePermissionRequest, DataScopeListParams, PermissionListParams, SortDir, UpdatePermissionRequest,
    };
    use entities::access_control::{AuditEventResult, DataScopeSubjectType, DataScopeType};
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields() {
        assert!(normalize_sort(&Some("actor_id".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) = normalize_sort(
            &Some(" updated_at ".to_string()),
            &None,
            &["created_at", "updated_at"],
        )
        .unwrap();
        assert_eq!(field, "updated_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn permission_list_params_normalize_and_validate() {
        let params = PermissionListParams {
            resource: Some(" sales_order ".to_string()),
            disabled: Some(false),
            system: Some(true),
            page: Some(2),
            page_size: Some(50),
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.resource.as_deref(), Some("sales_order"));
        assert_eq!(query.system, Some(true));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);

        let invalid = PermissionListParams {
            resource: None,
            disabled: None,
            system: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn data_scope_list_params_require_subject_type_for_subject_query() {
        let params = DataScopeListParams {
            subject_type: Some(DataScopeSubjectType::Role),
            scope_type: None,
            subject_id: Some(" role-sales ".to_string()),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.subject_id.as_deref(), Some("role-sales"));

        let missing = DataScopeListParams {
            subject_type: None,
            scope_type: None,
            subject_id: Some("role-sales".to_string()),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        assert!(missing.normalized().is_err());
    }

    #[test]
    fn permission_create_request_keeps_fields() {
        let request: CreatePermissionRequest = serde_json::from_value(json!({
            "resource": "sales_order",
            "action": "approve",
            "name": "销售单审批",
        }))
        .unwrap();
        assert!(!request.system);
        let data = request.into_data();
        assert_eq!(data.resource, "sales_order");
    }

    #[test]
    fn permission_update_reports_changed_fields_in_contract_order() {
        let empty = UpdatePermissionRequest {
            version: 1,
            name: None,
            description: None,
            disabled: None,
        };
        assert!(empty.changed_field_names().is_empty());

        let complete = UpdatePermissionRequest {
            version: 1,
            name: Some("销售单审批".to_string()),
            description: Some(String::new()),
            disabled: Some(false),
        };
        assert_eq!(
            complete.changed_field_names(),
            vec!["name", "description", "disabled"]
        );
    }

    #[test]
    fn data_scope_create_request_converts() {
        let request: CreateDataScopeRequest = serde_json::from_value(json!({
            "subject_type": "role",
            "subject_id": "role-sales",
            "scope_type": "team",
            "scope_targets": ["team-1", "team-2"],
        }))
        .unwrap();
        let data = request.into_data();
        assert_eq!(data.subject_type, DataScopeSubjectType::Role);
        assert_eq!(data.scope_type, DataScopeType::Team);
        assert_eq!(data.scope_targets, vec!["team-1", "team-2"]);
    }

    #[test]
    fn user_role_assign_request_defaults_effective_from_now() {
        let request: AssignUserRoleRequest = serde_json::from_value(json!({
            "user_id": "user-1",
            "role_id": "role-sales",
            "effective_to": 1700604800,
        }))
        .unwrap();
        let data = request.into_data("admin-1");
        assert_eq!(data.user_id, "user-1");
        assert_eq!(data.assigned_by, "admin-1");
        assert_eq!(
            data.effective_from.unix_secs(),
            entities::common::time::Instant::now().unix_secs()
        );
        assert!(data.effective_to.is_some());
    }

    #[test]
    fn audit_event_list_params_normalize() {
        let params = AuditEventListParams {
            actor_id: Some(" user-1 ".to_string()),
            action_type: Some("permission.create".to_string()),
            object_type: None,
            object_id: None,
            result: Some(AuditEventResult::Denied),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.actor_id.as_deref(), Some("user-1"));
        assert_eq!(query.result, Some(AuditEventResult::Denied));
        assert_eq!(query.paging.page_size, 20);
    }
}
