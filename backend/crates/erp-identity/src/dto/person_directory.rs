//! 人员查询目录的协议对象。类别由路由固定，客户端不能传入任意资源或权限。

use application_core::{PageView, QueryIds};
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::entity::account_core::AccountStatus;
use crate::entity::person_directory::DirectoryListRequest;

/// 目录列表查询。未知字段由反序列化拒绝。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonDirectoryQuery {
    /// 姓名或登录账号搜索。
    pub q: Option<String>,
    /// 页码。
    pub page: Option<u64>,
    /// 页大小。
    pub page_size: Option<u32>,
    /// 组织筛选。
    pub org_unit_ids: Option<QueryIds>,
    /// 展开筛选组织的下级。
    pub include_descendants: Option<bool>,
    /// 翻页时携带的目录授权版本。
    pub scope_version: Option<String>,
}

impl PersonDirectoryQuery {
    /// 转成目录服务使用的规范化条件。
    ///
    /// # 返回
    /// 返回校验后的查询条件。
    ///
    /// # 错误
    /// 搜索、分页、组织筛选或版本不合法时返回校验错误。
    pub fn into_request(self) -> Result<DirectoryListRequest> {
        let org_unit_ids = self.org_unit_ids.map(|ids| ids.as_slice().to_vec()).unwrap_or_default();
        DirectoryListRequest::parse(
            self.q.as_deref(),
            self.page,
            self.page_size,
            &org_unit_ids,
            self.include_descendants.unwrap_or(false),
            self.scope_version.as_deref(),
        )
    }
}

/// 已选人员回显查询。`ids` 必填。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonDirectorySelectedQuery {
    /// 已选账号 ID，最多 100 个。
    pub ids: QueryIds,
}

/// 目录中的一个人员。不含密码、联系方式或账号管理操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonDirectoryItem {
    /// 账号稳定 ID。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 登录账号，用于区分同名人员。
    pub account: String,
    /// 账号状态。停用账号仍可出现在有效查询资格中。
    pub status: AccountStatus,
    /// 授权时点的主属组织名称；没有组织时为空。
    pub org_label: Option<String>,
}

/// 人员目录页。版本独立于业务列表。
#[derive(Debug, Clone, Serialize)]
pub struct PersonDirectoryPage {
    /// 当前页人员。
    pub items: Vec<PersonDirectoryItem>,
    /// 同一资格、搜索和授权条件下的总数。
    pub total: i64,
    /// 当前页码。
    pub page: u64,
    /// 页大小。
    pub page_size: u32,
    /// 目录授权与可见成员版本。
    pub scope_version: String,
    /// 权限策略版本。
    pub policy_version: u64,
    /// 组织版本。
    pub organization_version: u64,
    /// 本次授权时点。
    pub as_of: String,
    /// 无有效范围时为 `no_scope`；搜索无结果时为空。
    pub empty_reason: Option<&'static str>,
}

impl PersonDirectoryPage {
    /// 由分页结果和授权元信息组装目录响应。
    ///
    /// # 参数
    /// * `page` - 已按授权过滤的人员页
    /// * `scope_version` - 目录版本
    /// * `policy_version` - 策略版本
    /// * `organization_version` - 组织版本
    /// * `as_of` - 授权时点
    /// * `empty_reason` - 空结果原因
    ///
    /// # 返回
    /// 返回目录页。
    pub fn from_page(
        page: PageView<PersonDirectoryItem>,
        scope_version: String,
        policy_version: u64,
        organization_version: u64,
        as_of: String,
        empty_reason: Option<&'static str>,
    ) -> Self {
        Self {
            items: page.items,
            total: page.total,
            page: page.page,
            page_size: page.page_size,
            scope_version,
            policy_version,
            organization_version,
            as_of,
            empty_reason,
        }
    }
}
