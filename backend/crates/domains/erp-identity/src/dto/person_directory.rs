//! 人员查询目录的协议对象。类别由路由固定，客户端不能传入任意资源或权限。

use application_core::{PageView, QueryIds};
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::entity::account_core::AccountStatus;
use crate::entity::person_directory::DirectoryListRequest;

/// 按路由固定的类别查询人员目录；拒绝未知字段。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonDirectoryQuery {
    /// 按姓名或登录账号作字面量搜索，最多 200 个字符。
    pub q: Option<String>,
    /// 请求页码，从 1 起，未提供时取 1。
    pub page: Option<u64>,
    /// 每页条数为 1 至 100，未提供时取 20。
    pub page_size: Option<u32>,
    /// 用于收窄候选的组织 ID，不扩大目录授权。
    pub org_unit_ids: Option<QueryIds>,
    /// 是否纳入筛选组织的下级；默认否，指定组织后才可启用。
    pub include_descendants: Option<bool>,
    /// 第 2 页起必带的目录授权版本，最多 256 个字符。
    pub scope_version: Option<String>,
}

impl PersonDirectoryQuery {
    /// 目录查询前将原始参数转成服务条件；搜索、分页、组织筛选或版本非法时拒绝。
    pub fn into_request(self) -> Result<DirectoryListRequest> {
        let org_unit_ids = self.org_unit_ids.as_ref().map_or(&[][..], QueryIds::as_slice);
        DirectoryListRequest::parse(
            self.q.as_deref(),
            self.page,
            self.page_size,
            org_unit_ids,
            self.include_descendants.unwrap_or(false),
            self.scope_version.as_deref(),
        )
    }
}

/// 回显已选人员时按指定 ID 查询，仍受当前目录授权约束。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonDirectorySelectedQuery {
    /// 必填的已选账号 ID，最多 100 个。
    pub ids: QueryIds,
}

/// 供人员选择与回显的目录项，不含密码、联系方式或账号管理操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonDirectoryItem {
    /// 表单保存人员引用时使用的稳定账号 ID。
    pub id: String,
    /// 用于展示和搜索的人员名称。
    pub name: String,
    /// 用于搜索及区分同名人员的登录账号。
    pub account: String,
    /// 当前账号状态；查询资格有效时，停用账号仍可展示。
    pub status: AccountStatus,
    /// 授权时点的主属组织名称；无主属组织时为空。
    pub org_label: Option<String>,
    /// 授权时点的有效主属组织 ID；仅用于表单带值，写入前须重验。
    #[serde(default)]
    pub primary_org_unit_id: Option<String>,
}

/// 供列表展示和后续翻页使用的授权人员目录页，版本独立于业务列表。
#[derive(Debug, Clone, Serialize)]
pub struct PersonDirectoryPage {
    /// 通过当前目录授权及查询筛选的本页人员。
    pub items: Vec<PersonDirectoryItem>,
    /// 按同一资格、搜索和授权条件计算的总数。
    pub total: i64,
    /// 本页的页码，从 1 起。
    pub page: u64,
    /// 本次查询使用的单页上限。
    pub page_size: u32,
    /// 目录授权与可见成员的版本，供后续页校验。
    pub scope_version: String,
    /// 本页使用的权限策略版本。
    pub policy_version: u64,
    /// 本页使用的组织配置版本。
    pub organization_version: u64,
    /// 解析本页目录授权的时点。
    pub as_of: String,
    /// 无有效范围时为 `no_scope`；有范围但搜索无结果时为空。
    pub empty_reason: Option<&'static str>,
}

impl PersonDirectoryPage {
    /// 目录查询返回前，将已授权分页结果和同次读取的范围版本、时点及空集原因一并返回。
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
