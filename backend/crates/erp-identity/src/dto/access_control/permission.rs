//! 域 D06 `access_control` 的 权限定义 DTO。

use application_core::{non_blank, normalized_text};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::PageParams;
use crate::entity::access_control::{Permission, PermissionData, PermissionUpdate};
use crate::error::Result;

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

impl From<crate::repository::PermissionRow> for PermissionView {
    /// 从列表投影行构造响应视图（字段取值与实体转换一致）。
    fn from(row: crate::repository::PermissionRow) -> Self {
        Self {
            id: row.id,
            resource: row.resource,
            action: row.action,
            name: row.name,
            description: row.description,
            system: row.system,
            disabled: row.disabled,
            version: row.version,
            created_at: row.created_at,
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
    pub fn into_data(self) -> PermissionData {
        let data = PermissionData::new(self.resource, self.action, self.name).with_system(self.system);
        match self.description {
            Some(description) => data.with_description(description),
            None => data,
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
    pub fn changed_field_names(&self) -> Vec<String> {
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
    pub fn into_update(self) -> PermissionUpdate {
        PermissionUpdate { name: self.name, description: self.description, disabled: self.disabled }
    }
}

/// 权限定义列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
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
        Ok(PermissionListQuery {
            resource: normalized_text(self.resource.as_deref()),
            disabled: self.disabled,
            system: self.system,
            paging: super::page_params(&self.sort_by, &self.sort_dir, self.page, self.page_size)?,
        })
    }
}
