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

/// 已登记的人员目录类别，稳定代码独立于角色显示名、建单和交接权限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonDirectoryCategory {
    /// 销售人员目录，自动补记资格只认 `role-sales`。
    Sales,
    /// 采购负责人目录，自动补记资格只认 `role-procurement`。
    Procurement,
    /// 通用后台人员目录，无需岗位资格记录。
    Business,
}

impl PersonDirectoryCategory {
    /// 返回接口与持久化共用的类别代码：`sales`、`procurement` 或 `business`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sales => "sales",
            Self::Procurement => "procurement",
            Self::Business => "business",
        }
    }

    /// 返回目录数据范围的资源代码：销售为 `sales_person`，采购为 `procurement_person`，后台人员为 `business_person`。
    pub fn resource(self) -> &'static str {
        match self {
            Self::Sales => "sales_person",
            Self::Procurement => "procurement_person",
            Self::Business => "business_person",
        }
    }

    /// 返回自动授予时观察的稳定角色 ID：销售为 `role-sales`，采购为 `role-procurement`，后台人员为 `None`。
    /// 销售领导角色、建单权限和业务单据归属均不触发自动授予。
    pub fn grant_role_id(self) -> Option<&'static str> {
        match self {
            Self::Sales => Some("role-sales"),
            Self::Procurement => Some("role-procurement"),
            Self::Business => None,
        }
    }

    /// 由本次分配的角色识别可自动补记资格的目录。
    /// 仅 `role-sales` 和 `role-procurement` 匹配，通用后台人员目录无对应角色。
    pub fn from_grant_role(role_id: &str) -> Option<Self> {
        match role_id {
            "role-sales" => Some(Self::Sales),
            "role-procurement" => Some(Self::Procurement),
            _ => None,
        }
    }
}

/// 岗位目录资格状态；终止后须显式再授予，角色保留和初始化重跑都不会恢复资格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonQueryStatus {
    /// 满足对应岗位目录的资格条件，实际可见性仍由查询授权和筛选决定。
    Active,
    /// 已结束该岗位目录资格，再授予前目录排除此账号。
    Terminated,
}

impl PersonQueryStatus {
    /// 返回与序列化一致的状态代码：有效为 `active`，已终止为 `terminated`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Terminated => "terminated",
        }
    }
}

/// 自动补记前查到的资格记录状态，用于区分首次授予和既有记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingGrant {
    /// 账号在该目录尚无记录，可补记首次授予。
    Absent,
    /// 资格已有效，保留现有记录。
    Active,
    /// 资格已终止，保留终止记录。
    Terminated,
}

/// 自动授予时仅为尚无记录的账号和目录组合补记资格，保留有效及已终止的记录。
/// 此判定只接受记录状态；角色撤销不删除资格，初始化重跑不恢复已终止资格。
pub fn should_insert_grant(existing: ExistingGrant) -> bool {
    matches!(existing, ExistingGrant::Absent)
}

/// 岗位目录的人员查询资格；每个账号和目录组合至多保留一条记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct PersonQueryQualification {
    /// 资格记录 ID 与持久化元数据，版本用于显式维护的并发校验。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 获授目录资格的后台账号稳定 ID。
    pub account_id: String,
    /// 此资格对应的岗位目录类别。
    pub category: PersonDirectoryCategory,
    /// 此账号的目录资格当前有效或已终止。
    pub status: PersonQueryStatus,
    /// 自动首次授予时记录的稳定角色 ID；首次由人工授予时为空。
    pub grant_role_id: Option<String>,
    /// 最近一次显式维护的操作人 ID；仅自动补记时为空。
    #[serde(default)]
    pub managed_by: Option<String>,
    /// 最近一次显式维护的审计原因，尚无显式维护时为空。
    #[serde(default)]
    pub management_reason: Option<String>,
    /// 当前终止状态的生效时点，有效时为 `None`。
    pub terminated_at: Option<Instant>,
}

impl PersonQueryQualification {
    /// 构造尚未终止的岗位目录资格，授予角色取类别固定的稳定 ID。
    /// `Business` 无需岗位资格，账号 ID 须含非空白字符；不满足时返回 `Error::ValidationError`。
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

    /// 将目录资格标记为 `Terminated` 并记录终止时点 `at`，保留账号角色。
    /// 重复终止返回 `Error::ConflictError`。
    pub fn terminate(&mut self, at: Instant) -> Result<()> {
        if self.status == PersonQueryStatus::Terminated {
            return Err(Error::ConflictError("查询资格已终止".into()));
        }
        self.status = PersonQueryStatus::Terminated;
        self.terminated_at = Some(at);
        Ok(())
    }
}

/// 规范化后的人员目录分页条件；组织筛选只收窄授权范围内的人员。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryListRequest {
    /// 对姓名或登录账号执行字面量匹配的搜索词，空白视为未指定。
    pub search: Option<String>,
    /// 待读取的页码，从 1 开始，默认第 1 页。
    pub page: u64,
    /// 每页条数，默认 20，允许 1 至 100。
    pub page_size: u32,
    /// 用于筛选人员主属关系的组织 ID，空集合表示不按组织筛选。
    pub org_unit_ids: Vec<String>,
    /// 是否将所选组织的下级纳入筛选，须先指定组织。
    pub include_descendants: bool,
    /// 客户端续页时回传的目录授权版本，用于检测查询条件或可见范围变化。
    pub scope_version: Option<String>,
}

impl DirectoryListRequest {
    /// 规范化搜索词和范围版本（分别最多 200 和 256 字符），复制调用方已去重的组织 ID。
    /// 页码默认为 1，页大小默认为 20；拒绝第 0 页、页大小超出 1 至 100、续页缺少版本和无组织却要求展开下级。
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

/// 判断已具备目录资格的后台账号是否同时满足操作人的列表数据范围与组织筛选。
/// `scope` 为对应目录已解析的列表范围，如 `sales_person:list` 或 `procurement_person:list`；本人按账号 ID 相等判定。
/// 组织维度取授权时点的主属组织，`org_filter` 为已展开的筛选集合或 `None`；无主属组织时只可能命中公司或本人范围。
/// 协作和历史参与恒为否，业务单据关系不会扩大目录可见性。
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

// 搜索词去除首尾空白，空白视为缺省，超过 200 个字符时报错。
fn normalize_search(search: Option<&str>) -> Result<Option<String>> {
    let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if search.chars().count() > SEARCH_MAX_CHARS {
        return Err(Error::ValidationError("搜索词最多 200 个字符".into()));
    }
    Ok(Some(search.to_string()))
}

// 范围版本去除首尾空白，空白视为缺省，超过 256 个字符时报错。
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

    // 构造无个人上限的公司范围，供目录可见性测试复用。
    fn company() -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..ScopeClause::default() }],
            user_limit: None,
        }
    }

    // 验证销售自动授予只认固定销售角色，采购目录使用独立的数据范围资源代码。
    #[test]
    fn sales_grant_role_is_not_the_leader_or_create_permission() {
        assert_eq!(PersonDirectoryCategory::Sales.grant_role_id(), Some("role-sales"));
        assert_eq!(PersonDirectoryCategory::from_grant_role("role-sales-leader"), None);
        assert_eq!(PersonDirectoryCategory::from_grant_role("sales_order:create"), None);
        assert_eq!(PersonDirectoryCategory::Procurement.resource(), "procurement_person");
    }

    // 验证自动授予只补缺失记录，有效资格可终止一次，已终止记录不会被自动恢复。
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

    // 验证公司范围可纳入具备资格但没有主属组织或业务归属的人员。
    #[test]
    fn company_scope_includes_a_qualified_person_without_org_or_business() {
        assert!(candidate_in_directory(&company(), "manager", "sales-b", None, None));
    }

    // 验证本人范围只可读自己，个人上限也能把公司范围收窄至本人。
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

    // 验证仅有协作范围时，人员目录不会纳入候选账号。
    #[test]
    fn collaborative_scope_never_adds_a_person() {
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { collaborative: true, ..ScopeClause::default() }],
            user_limit: None,
        };
        assert!(!candidate_in_directory(&scope, "leader", "sales-a", Some("org-1"), None));
    }

    // 验证组织授权和请求筛选均按主属组织匹配，筛选可收窄公司范围。
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

    // 验证搜索长度、分页边界、续页版本、下级展开的前置条件及默认分页。
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
