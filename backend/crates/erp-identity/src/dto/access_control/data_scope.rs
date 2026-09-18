//! 域 D06 `access_control` 的 数据范围 DTO。

use application_core::{non_blank, normalized_text};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{PageParams, PageView};
use crate::entity::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
use crate::error::{Error, Result};

/// 数据范围响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DataScopeView {
    /// 版本 2 资源动作绑定。
    #[serde(flatten)]
    pub binding: crate::access_control::ScopeBinding,
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
            binding: scope.binding,
            version: scope.base.version,
            created_at: scope.base.created_at,
        }
    }
}

impl From<crate::repository::DataScopeRow> for DataScopeView {
    /// 从列表投影行构造响应视图（字段取值与实体转换一致）。
    fn from(row: crate::repository::DataScopeRow) -> Self {
        Self {
            id: row.id,
            subject_type: row.subject_type,
            subject_id: row.subject_id,
            scope_type: row.scope_type,
            scope_targets: row.scope_targets,
            binding: row.binding,
            version: row.version,
            created_at: row.created_at,
        }
    }
}

/// 范围配置列表信封；版本与空集字段与组织查询同口径。
#[derive(Debug, Clone, Serialize)]
pub struct DataScopeListView {
    /// 当前页配置项；不含版本元数据。
    #[serde(flatten)]
    pub page: PageView<DataScopeView>,
    /// 本次列表范围版本。
    pub scope_version: String,
    /// 当前权限策略版本。
    pub policy_version: u64,
    /// 当前组织版本。
    pub organization_version: u64,
    /// 解析时点（RFC3339 UTC）。
    pub as_of: String,
    /// 角色缺范围时为 `no_scope`；有规则但无配置时为空。
    pub empty_reason: Option<&'static str>,
    /// 面向客户端的范围摘要，不含内部证明。
    pub scope_summary: &'static str,
    /// 范围配置归属口径。
    pub ownership_basis: &'static str,
}

/// 范围配置列表信封所需的版本与空集字段。
pub struct DataScopeListMeta {
    /// 本次列表范围版本。
    pub scope_version: String,
    /// 当前权限策略版本。
    pub policy_version: u64,
    /// 当前组织版本。
    pub organization_version: u64,
    /// 解析时点（RFC3339 UTC）。
    pub as_of: String,
    /// 角色是否缺少该动作范围。
    pub no_scope: bool,
}

impl DataScopeListMeta {
    /// 由必填范围版本与解析时点构造列表元数据。
    ///
    /// # 参数
    /// * `scope_version` - 本次列表范围版本
    /// * `as_of` - 解析时点（RFC3339 UTC）
    ///
    /// # 返回
    /// 返回策略/组织版本为零、非空集的列表元数据。
    ///
    /// # 错误
    /// 无。
    pub fn new(scope_version: impl Into<String>, as_of: impl Into<String>) -> Self {
        Self {
            scope_version: scope_version.into(),
            policy_version: 0,
            organization_version: 0,
            as_of: as_of.into(),
            no_scope: false,
        }
    }

    /// 设置当前权限策略版本。
    ///
    /// # 参数
    /// * `policy_version` - 当前权限策略版本
    ///
    /// # 返回
    /// 返回更新后的列表元数据。
    ///
    /// # 错误
    /// 无。
    pub fn with_policy_version(mut self, policy_version: u64) -> Self {
        self.policy_version = policy_version;
        self
    }

    /// 设置当前组织版本。
    ///
    /// # 参数
    /// * `organization_version` - 当前组织版本
    ///
    /// # 返回
    /// 返回更新后的列表元数据。
    ///
    /// # 错误
    /// 无。
    pub fn with_organization_version(mut self, organization_version: u64) -> Self {
        self.organization_version = organization_version;
        self
    }

    /// 设置角色缺范围标记。
    ///
    /// # 参数
    /// * `no_scope` - 角色是否缺少该动作范围
    ///
    /// # 返回
    /// 返回更新后的列表元数据。
    ///
    /// # 错误
    /// 无。
    pub fn with_no_scope(mut self, no_scope: bool) -> Self {
        self.no_scope = no_scope;
        self
    }
}

impl DataScopeListView {
    /// 组合范围配置列表信封。
    ///
    /// # 参数
    /// * `page` - 当前页配置项与分页计数
    /// * `meta` - 范围、权限、组织版本与空集标记
    ///
    /// # 返回
    /// 与组织查询同口径的列表信封。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 版本与空集原因只出现在信封上，不得写入单条配置。缺范围标记 `no_scope`，不得补 Company。
    pub fn compose(page: PageView<DataScopeView>, meta: DataScopeListMeta) -> Self {
        Self {
            page,
            scope_version: meta.scope_version,
            policy_version: meta.policy_version,
            organization_version: meta.organization_version,
            as_of: meta.as_of,
            empty_reason: meta.no_scope.then_some("no_scope"),
            scope_summary: "已接入 DataScope v2 的资源动作配置",
            ownership_basis: "data_scope_configuration",
        }
    }
}

/// 数据范围创建请求（主体 + 范围类型唯一）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateDataScopeRequest {
    /// 版本 2 资源动作绑定。
    #[serde(flatten)]
    pub binding: crate::access_control::ScopeBinding,
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
    pub fn into_data(self) -> DataScopeData {
        DataScopeData {
            subject_type: self.subject_type,
            subject_id: self.subject_id,
            scope_type: self.scope_type,
            scope_targets: self.scope_targets,
            binding: self.binding,
        }
    }
}

/// 数据范围列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct DataScopeListParams {
    /// 范围主体类型筛选。
    pub subject_type: Option<DataScopeSubjectType>,
    /// 范围类型筛选。
    pub scope_type: Option<DataScopeType>,
    /// 范围主体 ID 筛选（与 `subject_type` 成对使用，走按主体查询）。
    pub subject_id: Option<String>,
    /// 资源筛选（已注册标识，禁止通配与显示名）。
    pub resource: Option<String>,
    /// 动作筛选（已注册标识，禁止通配与显示名）。
    pub action: Option<String>,
    /// 跨页携带的范围版本；缺省表示首页。
    pub scope_version: Option<String>,
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
    /// 资源筛选。
    pub resource: Option<String>,
    /// 动作筛选。
    pub action: Option<String>,
    /// 跨页携带的范围版本。
    pub scope_version: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl DataScopeListParams {
    /// 归一化数据范围列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验；按主体查询
    /// 时必须同时提供 `subject_type`。资源与动作必须是注册标识。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单、方向非法、按主体查询缺少 `subject_type`，或资源
    /// 动作使用通配/显示名时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<DataScopeListQuery> {
        let subject_id = normalized_text(self.subject_id.as_deref());
        if subject_id.is_some() && self.subject_type.is_none() {
            return Err(Error::ValidationError("按主体查询时必须提供范围主体类型".to_string()));
        }
        Ok(DataScopeListQuery {
            subject_type: self.subject_type,
            scope_type: self.scope_type,
            subject_id,
            resource: registered_identifier(self.resource.as_deref(), "资源")?,
            action: registered_identifier(self.action.as_deref(), "动作")?,
            scope_version: normalized_text(self.scope_version.as_deref()),
            paging: super::page_params(&self.sort_by, &self.sort_dir, self.page, self.page_size)?,
        })
    }
}

/// 规范化范围配置使用的注册标识。
///
/// # 参数
/// * `value` - 原始筛选文本
/// * `field` - 字段中文名，用于错误提示
///
/// # 返回
/// 空白视为未筛选；非空时返回去空白后的标识。
///
/// # 错误
/// 通配符、显示名或非法字符时返回校验错误。
fn registered_identifier(value: Option<&str>, field: &str) -> Result<Option<String>> {
    let Some(text) = normalized_text(value) else {
        return Ok(None);
    };
    if text.is_empty()
        || text.len() > 128
        || !text.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(Error::ValidationError(format!("{field}必须使用已注册标识，禁止通配符和显示名")));
    }
    Ok(Some(text))
}
