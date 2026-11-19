//! 管理端权限目录的查询和写入参数，以及权限定义响应。

use application_core::{non_blank, normalized_text};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::PageParams;
use crate::entity::access_control::{PermissionData, PermissionDefinition, PermissionUpdate};
use crate::error::Result;
use crate::repository::PermissionFilter;

/// 管理端权限目录（W19）的单项定义，供列表和写入响应共用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionView {
    /// 权限定义 ID。
    pub id: String,
    /// 规范化后的资源代码。
    pub resource: String,
    /// 规范化后的动作代码。
    pub action: String,
    /// 权限在界面中的显示名称。
    pub name: String,
    /// 权限用途说明（若有）。
    pub description: Option<String>,
    /// 标记系统内建权限；内建定义禁止删除。
    pub system: bool,
    /// 当前定义是否标记为停用。
    pub disabled: bool,
    /// 当前定义版本，编辑时用作乐观锁。
    pub version: u64,
    /// 定义创建时间，以 Unix 秒记录。
    pub created_at: u64,
}

impl From<PermissionDefinition> for PermissionView {
    // 从实体生成目录响应，使用权限值中已规范化的资源与动作。
    fn from(permission: PermissionDefinition) -> Self {
        let resource = permission.permission().resource().to_owned();
        let action = permission.permission().action().to_owned();
        Self {
            id: permission.base.id,
            resource,
            action,
            name: permission.name,
            description: permission.description,
            system: permission.system,
            disabled: permission.disabled,
            version: permission.base.version,
            created_at: permission.base.created_at,
        }
    }
}

/// 创建权限定义；实体构造时用 `Permission::parse` 规范化 `resource:action`。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePermissionRequest {
    /// 非空白资源代码，例如 `sales_order`。
    #[validate(custom(function = "non_blank", message = "权限资源不能为空"))]
    pub resource: String,
    /// 非空白动作代码，例如 `approve`。
    #[validate(custom(function = "non_blank", message = "权限动作不能为空"))]
    pub action: String,
    /// 显示名称须含非空白内容；实体校验裁剪后的长度，上限为 64 个字符。
    #[validate(custom(function = "non_blank", message = "权限名称不能为空"))]
    pub name: String,
    /// 可选用途说明，最多 256 个字符。
    #[validate(length(max = 256, message = "权限描述过长"))]
    pub description: Option<String>,
    /// 是否标记为系统内建；默认 `false`。内建标记创建后固定，内建定义禁止删除。
    #[serde(default)]
    pub system: bool,
}

impl CreatePermissionRequest {
    /// 创建权限定义前将请求转为实体输入；未提供描述时不设置说明。
    pub fn into_data(self) -> PermissionData {
        let data = PermissionData::new(self.resource, self.action, self.name).with_system(self.system);
        match self.description {
            Some(description) => data.with_description(description),
            None => data,
        }
    }
}

/// 带乐观锁的权限定义更新请求，版本冲突返回 HTTP 409。
/// 资源、动作和内建标记固定；内建定义也可更新名称、说明和停用标记。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePermissionRequest {
    /// 请求期望的当前版本，最小为 1。
    #[validate(range(min = 1, message = "资料版本无效，请刷新页面后重试"))]
    pub version: u64,
    /// 新的显示名称；省略时保留原值，提供时由实体校验。
    pub name: Option<String>,
    /// 新的用途说明；省略时保留原值，提供空白文本时清空。
    pub description: Option<String>,
    /// 新的停用标记；省略时保留原值。
    pub disabled: Option<bool>,
}

impl UpdatePermissionRequest {
    /// 供审计按 `name`、`description`、`disabled` 的固定顺序列出补丁中有值的字段名。
    /// 仅判断是否提供值；无可更新字段时返回空列表。
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

    /// 将可更新字段转为实体补丁；版本由服务另行校验。
    pub fn into_update(self) -> PermissionUpdate {
        PermissionUpdate { name: self.name, description: self.description, disabled: self.disabled }
    }
}

/// 权限目录的筛选和分页参数，按同一层级传入。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct PermissionListParams {
    /// 按资源代码做不区分大小写的字面量匹配。
    pub resource: Option<String>,
    /// 精确匹配停用标记；省略时包含两种状态。
    pub disabled: Option<bool>,
    /// 精确匹配内建标记：`true` 查内建权限，`false` 查自定义权限，省略时两者均查。
    pub system: Option<bool>,
    /// 页码，从 1 开始；默认第 1 页。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数，范围为 1–100；默认 20 条。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段，仅支持 `created_at` 和 `updated_at`；默认 `created_at`。
    pub sort_by: Option<String>,
    /// 排序方向，接受 `asc` 或 `desc`；默认降序。
    pub sort_dir: Option<String>,
}

impl PermissionListParams {
    /// 裁剪资源筛选项，忽略纯空白值，补齐分页和排序默认值后生成仓储筛选条件。
    /// 分页越界、排序字段超出白名单或方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<PermissionFilter> {
        Ok(PermissionFilter {
            resource: normalized_text(self.resource.as_deref()),
            disabled: self.disabled,
            system: self.system,
            paging: PageParams::parse(&self.sort_by, &self.sort_dir, self.page, self.page_size)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{deserialize_from_document, serialize_to_document};

    use super::*;
    use crate::access_control::PermissionId;

    // 验证 BSON 投影与实体转换一致，资源和动作已规范化，且缺少资源字段时解码失败。
    #[test]
    fn projection_and_entity_conversion_share_the_response_contract() {
        let entity = PermissionDefinition::new(
            PermissionId::new("permission-1"),
            PermissionData::new("Sales_Order", "Detail", "查看销售单").with_description("说明"),
        )
        .unwrap();
        let document = serialize_to_document(&entity).unwrap();
        let projection: PermissionView = deserialize_from_document(document.clone()).unwrap();
        assert_eq!(projection, PermissionView::from(entity));
        assert_eq!(projection.resource, "sales_order");
        assert_eq!(projection.action, "detail");

        let mut incomplete = document;
        incomplete.remove("resource");
        assert!(deserialize_from_document::<PermissionView>(incomplete).is_err());
    }
}
