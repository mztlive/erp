//! 人员查询目录的资格与可见性。资格定义目录成员，DataScope 定义操作人能读到的子集。

use std::collections::BTreeSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use crate::entity::access_control::{ResolvedScope, ScopedObject};
use crate::{Error, Result};

const SEARCH_MAX_CHARS: usize = 200;
const SCOPE_VERSION_MAX_CHARS: usize = 256;

/// 已登记的人员查询类别。稳定代码不是角色显示名，也不等于建单或交接权限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonDirectoryCategory {
    /// 当前销售查询目录。授予来源仅为稳定角色 `role-sales`。
    Sales,
    /// 当前采购负责人查询目录。授予来源仅为稳定角色 `role-procurement`。
    Procurement,
    /// 通用后台人员查询，不依赖岗位资格。
    Business,
}

impl PersonDirectoryCategory {
    /// 返回持久化与接口使用的稳定类别代码。
    ///
    /// # 返回
    /// `sales` 或 `procurement`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sales => "sales",
            Self::Procurement => "procurement",
            Self::Business => "business",
        }
    }

    /// 返回该目录的 DataScope 资源代码。
    ///
    /// # 返回
    /// 销售为 `sales_person`，采购为 `procurement_person`。
    pub fn resource(self) -> &'static str {
        match self {
            Self::Sales => "sales_person",
            Self::Procurement => "procurement_person",
            Self::Business => "business_person",
        }
    }

    /// 返回首次授予查询资格所观察的稳定角色 ID。
    ///
    /// # 返回
    /// 销售为 `role-sales`，采购为 `role-procurement`。
    ///
    /// # 关键业务约束
    /// 销售领导、建单权限和业务单据归属都不能代替该角色 ID。
    pub fn grant_role_id(self) -> Option<&'static str> {
        match self {
            Self::Sales => Some("role-sales"),
            Self::Procurement => Some("role-procurement"),
            Self::Business => None,
        }
    }

    /// 按稳定角色 ID 识别会首次写入的查询类别。
    ///
    /// # 参数
    /// * `role_id` - 本次分配结果中的角色 ID
    ///
    /// # 返回
    /// 该角色是某目录的授予来源时返回类别；其余角色返回 `None`。
    pub fn from_grant_role(role_id: &str) -> Option<Self> {
        match role_id {
            "role-sales" => Some(Self::Sales),
            "role-procurement" => Some(Self::Procurement),
            _ => None,
        }
    }
}

/// 查询资格状态。终止后不会因角色仍在或初始化重跑而恢复。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonQueryStatus {
    /// 可进入对应人员目录。
    Active,
    /// 已终止，目录不再返回该账号。
    Terminated,
}

impl PersonQueryStatus {
    /// 返回与序列化一致的稳定状态代码。
    ///
    /// # 返回
    /// `active` 或 `terminated`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Terminated => "terminated",
        }
    }
}

/// 已有资格记录对“是否插入”的影响。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingGrant {
    /// 尚无记录。
    Absent,
    /// 查询资格仍有效。
    Active,
    /// 查询资格已终止。
    Terminated,
}

/// 尚未存在记录时才插入。已有有效或已终止记录都保持原样。
///
/// # 参数
/// * `existing` - 当前账号与类别的资格记录状态
///
/// # 返回
/// 需要新建有效资格时返回 `true`。
///
/// # 关键业务约束
/// 角色撤销不是本函数的输入，不能据此删除资格；初始化重跑也不能恢复已终止记录。
pub fn should_insert_grant(existing: ExistingGrant) -> bool {
    matches!(existing, ExistingGrant::Absent)
}

/// 人员查询资格。同一账号同一类别至多一条记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct PersonQueryQualification {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 账号稳定 ID。
    pub account_id: String,
    /// 查询类别。
    pub category: PersonDirectoryCategory,
    /// 资格状态。
    pub status: PersonQueryStatus,
    /// 首次授予时观察到的稳定角色 ID。
    pub grant_role_id: Option<String>,
    /// 最近一次显式维护人；自动补记时为空。
    #[serde(default)]
    pub managed_by: Option<String>,
    /// 显式维护原因。
    #[serde(default)]
    pub management_reason: Option<String>,
    /// 终止时间；未终止时为空。
    pub terminated_at: Option<Instant>,
}

impl PersonQueryQualification {
    /// 建立一条有效查询资格。
    ///
    /// # 参数
    /// * `id` - 资格记录 ID
    /// * `account_id` - 账号稳定 ID
    /// * `category` - 查询类别
    ///
    /// # 返回
    /// 返回尚未终止的资格记录。
    ///
    /// # 错误
    /// 账号 ID 为空时返回校验错误。
    ///
    /// # 关键业务约束
    /// 授予角色固定取类别的稳定角色 ID，调用方不能改写成显示名或其他权限。
    pub fn grant(
        id: impl Into<String>,
        account_id: impl Into<String>,
        category: PersonDirectoryCategory,
    ) -> Result<Self> {
        if category == PersonDirectoryCategory::Business {
            return Err(Error::ValidationError("后台人员目录不使用岗位资格记录".into()));
        }
        let id = id.into();
        let account_id = account_id.into();
        if account_id.trim().is_empty() {
            return Err(Error::ValidationError("查询资格账号不能为空".into()));
        }
        Ok(Self {
            base: BaseModel::new(id),
            account_id,
            category,
            status: PersonQueryStatus::Active,
            grant_role_id: category.grant_role_id().map(str::to_owned),
            managed_by: None,
            management_reason: None,
            terminated_at: None,
        })
    }

    /// 终止查询资格。重复终止拒绝，且不改变账号角色。
    ///
    /// # 参数
    /// * `at` - 终止时点
    ///
    /// # 错误
    /// 资格已经终止时返回冲突。
    pub fn terminate(&mut self, at: Instant) -> Result<()> {
        if self.status == PersonQueryStatus::Terminated {
            return Err(Error::ConflictError("查询资格已终止".into()));
        }
        self.status = PersonQueryStatus::Terminated;
        self.terminated_at = Some(at);
        Ok(())
    }
}

/// 已规范化的目录列表条件。组织筛选只收窄候选，不扩大授权。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryListRequest {
    /// 姓名或登录账号的字面量搜索。
    pub search: Option<String>,
    /// 从 1 开始的页码。
    pub page: u64,
    /// 页大小，最大 100。
    pub page_size: u32,
    /// 可选组织筛选。
    pub org_unit_ids: Vec<String>,
    /// 是否展开筛选组织的下级。
    pub include_descendants: bool,
    /// 后续页携带的目录授权版本。
    pub scope_version: Option<String>,
}

impl DirectoryListRequest {
    /// 校验并规范化目录查询参数。
    ///
    /// # 参数
    /// * `search` - 原始搜索词
    /// * `page` - 原始页码；缺省为 1
    /// * `page_size` - 原始页大小；缺省为 20
    /// * `org_unit_ids` - 已去重的组织 ID
    /// * `include_descendants` - 是否包含下级
    /// * `scope_version` - 客户端持有的目录版本
    ///
    /// # 返回
    /// 返回可执行的查询条件。
    ///
    /// # 错误
    /// 搜索超长、页码或页大小越界、版本超长，或未指定组织却要求包含下级时拒绝。
    pub fn parse(
        search: Option<&str>,
        page: Option<u64>,
        page_size: Option<u32>,
        org_unit_ids: &[String],
        include_descendants: bool,
        scope_version: Option<&str>,
    ) -> Result<Self> {
        let search = normalize_search(search)?;
        let scope_version = normalize_scope_version(scope_version)?;
        let page = page.unwrap_or(1);
        let page_size = page_size.unwrap_or(20);
        if page > 1 && scope_version.is_none() {
            return Err(Error::ValidationError("后续页必须携带目录范围版本".into()));
        }
        if page == 0 {
            return Err(Error::ValidationError("页码必须从 1 开始".into()));
        }
        if page_size == 0 || page_size > 100 {
            return Err(Error::ValidationError("页大小必须在 1 到 100 之间".into()));
        }
        if include_descendants && org_unit_ids.is_empty() {
            return Err(Error::ValidationError("只有指定组织时才能包含下级".into()));
        }
        Ok(Self {
            search,
            page,
            page_size,
            org_unit_ids: org_unit_ids.to_vec(),
            include_descendants,
            scope_version,
        })
    }
}

/// 判断单个已具备资格的后台账号是否落在操作人的目录授权和组织筛选内。
///
/// # 参数
/// * `scope` - 已解析的 `sales_person:list` 或 `procurement_person:list` 范围
/// * `actor_id` - 当前操作人账号 ID
/// * `account_id` - 候选账号 ID
/// * `primary_org` - 候选在授权时点的主属组织；没有组织时为 `None`
/// * `org_filter` - 请求指定并已展开的组织集合；未指定组织筛选时为 `None`
///
/// # 返回
/// 候选可读且满足组织筛选时返回 `true`。
///
/// # 关键业务约束
/// 协作参与和历史参与恒为否，不能借业务单据把人员读权限放大。
/// 无主属组织的人员只可能命中公司或本人范围。
pub fn candidate_in_directory(
    scope: &ResolvedScope,
    actor_id: &str,
    account_id: &str,
    primary_org: Option<&str>,
    org_filter: Option<&BTreeSet<String>>,
) -> bool {
    if org_filter.is_some_and(|filter| primary_org.is_none_or(|org_id| !filter.contains(org_id))) {
        return false;
    }
    scope.allows(
        &ScopedObject {
            owned: actor_id == account_id,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: primary_org,
            settlement_party_id: None,
            warehouse_id: None,
        },
        false,
    )
}

/// 计算需装载成员的有限组织集合；Company 无组织约束时无需枚举账号。
/// # 参数
/// `scope` 为角色并集及个人上限，`filter` 为已展开的候选组织筛选。
/// # 返回
/// 返回组织维度交集；本人身份由调用方另行判定，不由空集合推导公司范围。
/// # 错误
/// 无；输入范围已由公共解析器校验。
pub(crate) fn directory_org_ids(
    scope: &ResolvedScope,
    filter: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    let company = scope.role_clauses.iter().any(|clause| clause.company);
    let mut orgs = if company {
        scope.user_limit.as_ref().filter(|limit| !limit.company)
            .map(|limit| limit.org_unit_ids.clone())
            .unwrap_or_else(|| filter.cloned().unwrap_or_default())
    } else {
        scope.role_clauses.iter().flat_map(|clause| clause.org_unit_ids.iter().cloned()).collect()
    };
    if let Some(limit) = &scope.user_limit && !limit.company {
        orgs.retain(|id| limit.org_unit_ids.contains(id));
    }
    if let Some(filter) = filter { orgs.retain(|id| filter.contains(id)); }
    orgs
}

fn normalize_search(search: Option<&str>) -> Result<Option<String>> {
    let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if search.chars().count() > SEARCH_MAX_CHARS {
        return Err(Error::ValidationError("搜索词最多 200 个字符".into()));
    }
    Ok(Some(search.to_string()))
}

fn normalize_scope_version(scope_version: Option<&str>) -> Result<Option<String>> {
    let Some(version) = scope_version.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if version.chars().count() > SCOPE_VERSION_MAX_CHARS {
        return Err(Error::ValidationError("目录授权版本过长".into()));
    }
    Ok(Some(version.to_string()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use erp_core::common::time::Instant;

    use super::{
        DirectoryListRequest, ExistingGrant, PersonDirectoryCategory, PersonQueryQualification,
        PersonQueryStatus, candidate_in_directory, should_insert_grant,
    };
    use crate::entity::access_control::{ResolvedScope, ScopeClause};

    fn company() -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..ScopeClause::default() }],
            user_limit: None,
        }
    }

    #[test]
    fn sales_grant_role_is_not_the_leader_or_create_permission() {
        assert_eq!(PersonDirectoryCategory::Sales.grant_role_id(), Some("role-sales"));
        assert_eq!(PersonDirectoryCategory::from_grant_role("role-sales-leader"), None);
        assert_eq!(PersonDirectoryCategory::from_grant_role("sales_order:create"), None);
        assert_eq!(PersonDirectoryCategory::Procurement.resource(), "procurement_person");
    }

    #[test]
    fn missing_grant_inserts_once_and_termination_is_not_restored() {
        assert!(should_insert_grant(ExistingGrant::Absent));
        assert!(!should_insert_grant(ExistingGrant::Active));
        assert!(!should_insert_grant(ExistingGrant::Terminated));
        let mut grant =
            PersonQueryQualification::grant("q1", "sales-a", PersonDirectoryCategory::Sales).unwrap();
        assert_eq!(grant.status, PersonQueryStatus::Active);
        assert_eq!(grant.grant_role_id.as_deref(), Some("role-sales"));
        grant.terminate(Instant::from_unix_secs(10)).unwrap();
        assert!(grant.terminate(Instant::from_unix_secs(11)).is_err());
        assert_eq!(grant.status, PersonQueryStatus::Terminated);
    }

    #[test]
    fn company_scope_includes_a_qualified_person_without_org_or_business() {
        assert!(candidate_in_directory(&company(), "manager", "sales-b", None, None));
    }

    #[test]
    fn self_scope_and_user_limit_do_not_reveal_other_people() {
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..ScopeClause::default() }],
            user_limit: None,
        };
        assert!(candidate_in_directory(&scope, "sales-a", "sales-a", None, None));
        assert!(!candidate_in_directory(&scope, "sales-a", "sales-b", None, None));

        let limited = ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..ScopeClause::default() }],
            user_limit: Some(ScopeClause { self_owned: true, ..ScopeClause::default() }),
        };
        assert!(candidate_in_directory(&limited, "sales-a", "sales-a", None, None));
        assert!(!candidate_in_directory(&limited, "sales-a", "sales-b", None, None));
    }

    #[test]
    fn collaborative_scope_never_adds_a_person() {
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { collaborative: true, ..ScopeClause::default() }],
            user_limit: None,
        };
        assert!(!candidate_in_directory(&scope, "leader", "sales-a", Some("org-1"), None));
    }

    #[test]
    fn organization_scope_uses_primary_org_and_request_filter_only_narrows() {
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause {
                org_unit_ids: BTreeSet::from(["org-1".to_string()]),
                ..ScopeClause::default()
            }],
            user_limit: None,
        };
        assert!(candidate_in_directory(&scope, "leader", "sales-a", Some("org-1"), None));
        assert!(!candidate_in_directory(&scope, "leader", "sales-b", Some("org-2"), None));
        assert!(!candidate_in_directory(&scope, "leader", "sales-c", None, None));

        let filter = BTreeSet::from(["org-1".to_string()]);
        assert!(candidate_in_directory(&company(), "finance", "sales-a", Some("org-1"), Some(&filter)));
        assert!(!candidate_in_directory(&company(), "finance", "sales-b", Some("org-2"), Some(&filter)));
        assert!(!candidate_in_directory(&company(), "finance", "sales-c", None, Some(&filter)));
    }

    #[test]
    fn list_request_rejects_bounds_and_descendant_without_org() {
        assert!(DirectoryListRequest::parse(Some(" 张三 "), None, None, &[], false, Some("  ")).is_ok());
        assert!(
            DirectoryListRequest::parse(Some(&"人".repeat(201)), Some(1), Some(20), &[], false, None)
                .is_err()
        );
        assert!(DirectoryListRequest::parse(None, Some(2), Some(20), &[], false, None).is_err());
        assert!(DirectoryListRequest::parse(None, Some(2), Some(20), &[], false, Some(" ")).is_err());
        assert!(DirectoryListRequest::parse(None, Some(2), Some(20), &[], false, Some("v1")).is_ok());
        assert!(DirectoryListRequest::parse(None, Some(0), Some(20), &[], false, None).is_err());
        assert!(DirectoryListRequest::parse(None, Some(1), Some(101), &[], false, None).is_err());
        assert!(DirectoryListRequest::parse(None, Some(1), Some(20), &[], true, None).is_err());
        let request =
            DirectoryListRequest::parse(None, None, None, &["org-1".to_string()], true, None).unwrap();
        assert_eq!(request.page, 1);
        assert_eq!(request.page_size, 20);
        assert!(request.include_descendants);
    }
}
