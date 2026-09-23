//! 独立对象目录的分页协议与有界快照约定；不包含业务资格或授权政策。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Error, QueryIds, Result};

/// 目录有界快照上限；超出时整体拒绝。
pub const DIRECTORY_LIMIT: usize = 10_000;

/// 目录搜索或已选回显请求。
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryQuery {
    pub q: Option<String>,
    pub ids: Option<QueryIds>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
    pub scope_version: Option<String>,
}

/// 已选回显只接收请求中的身份集合。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorySelectedQuery {
    pub ids: QueryIds,
}
impl From<DirectorySelectedQuery> for DirectoryQuery {
    fn from(value: DirectorySelectedQuery) -> Self {
        Self { ids: Some(value.ids), ..Default::default() }
    }
}

impl DirectoryQuery {
    /// 校验搜索、分页与版本约束。
    /// # 参数
    /// `self` 为 HTTP 解码后的请求。
    /// # 返回
    /// 成功返回规范化请求。
    /// # 错误
    /// 非法页、空版本后续页或超长关键词时拒绝。
    pub fn normalized(mut self) -> Result<Self> {
        self.q = self.q.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        self.scope_version = self.scope_version.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        if self.q.as_ref().is_some_and(|q| q.chars().count() > 200)
            || self.page.unwrap_or(1) == 0
            || !(1..=100).contains(&self.page_size.unwrap_or(20))
            || self.scope_version.as_ref().is_some_and(|v| v.len() > 256)
            || (self.page.unwrap_or(1) > 1 && self.scope_version.is_none())
        {
            return Err(Error::ValidationError("目录搜索、分页或范围版本不合法".into()));
        }
        if self.ids.is_some() && (self.q.is_some() || self.page.unwrap_or(1) != 1) {
            return Err(Error::ValidationError("已选回显不能混用搜索和后续页".into()));
        }
        Ok(self)
    }
}

/// 目录轻量展示项。状态允许停用；是否可用于命令由命令另行判定。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryItem {
    pub id: String,
    pub code: String,
    pub name: String,
    pub status: String,
}

/// 消费方 Port 提供的授权身份集合及版本；None 表示经证明的公司范围。
#[derive(Clone)]
pub struct DirectoryScope {
    pub ids: Option<Vec<String>>,
    pub scope_version: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub as_of: String,
    pub no_scope: bool,
}

/// 独立目录响应，包含完整查询快照版本。
#[derive(Serialize)]
pub struct DirectoryPage {
    pub items: Vec<DirectoryItem>,
    pub total: usize,
    pub page: u64,
    pub page_size: u32,
    pub scope_version: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub as_of: String,
    pub empty_reason: Option<&'static str>,
}

impl DirectoryPage {
    /// 对有界授权结果稳定分页，内容与授权一起参与版本计算。
    /// # 参数
    /// `items` 为已完成资格、范围、搜索的完整有界结果，`query` 为规范化请求。
    /// # 返回
    /// 目录页；调用者必须比较请求与响应版本后才返回。
    /// # 错误
    /// 超出规模边界或序列化失败时拒绝。
    pub fn from_snapshot(
        mut items: Vec<DirectoryItem>,
        query: &DirectoryQuery,
        scope: DirectoryScope,
    ) -> Result<Self> {
        if items.len() > DIRECTORY_LIMIT {
            return Err(Error::ValidationError("目录超过查询上限，请收窄搜索条件".into()));
        }
        items.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        let mut hash = Sha256::new();
        hash.update(scope.scope_version.as_bytes());
        hash.update(serde_json::to_vec(&items).map_err(|e| Error::Internal(e.to_string()))?);
        let scope_version = hex::encode(hash.finalize());
        let total = items.len();
        let page = query.page.unwrap_or(1);
        let page_size = if query.ids.is_some() { 100 } else { query.page_size.unwrap_or(20) };
        let skip = page.saturating_sub(1).saturating_mul(u64::from(page_size));
        let skip = usize::try_from(skip).unwrap_or(usize::MAX);
        Ok(Self {
            items: items.into_iter().skip(skip).take(usize::try_from(page_size).unwrap_or(100)).collect(),
            total,
            page,
            page_size,
            scope_version,
            policy_version: scope.policy_version,
            organization_version: scope.organization_version,
            as_of: scope.as_of,
            empty_reason: scope.no_scope.then_some("no_scope"),
        })
    }
}

/// 业务列表范围元信息；不附带下拉候选。
#[derive(Serialize)]
pub struct ScopedPage<T> {
    #[serde(flatten)]
    pub page: crate::PageView<T>,
    pub scope_version: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub as_of: String,
    pub empty_reason: Option<&'static str>,
}
impl<T> ScopedPage<T> {
    /// 将已授权分页与范围元信息组合。
    /// # 参数
    /// `page` 为已应用范围条件的结果，`scope` 为同事务授权。
    /// # 返回
    /// 带范围元信息的列表。
    /// # 错误
    /// 无。
    pub fn new(page: crate::PageView<T>, scope: DirectoryScope) -> Self {
        Self {
            page,
            scope_version: scope.scope_version,
            policy_version: scope.policy_version,
            organization_version: scope.organization_version,
            as_of: scope.as_of,
            empty_reason: scope.no_scope.then_some("no_scope"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(version: &str) -> DirectoryScope {
        DirectoryScope {
            ids: None,
            scope_version: version.into(),
            policy_version: 1,
            organization_version: 1,
            as_of: "2026-09-23".into(),
            no_scope: false,
        }
    }
    fn item(id: &str, name: &str) -> DirectoryItem {
        DirectoryItem { id: id.into(), code: id.into(), name: name.into(), status: "active".into() }
    }
    #[test]
    fn later_pages_require_nonblank_version_and_selected_rejects_mixed_search() {
        for version in [None, Some(" ".into())] {
            assert!(
                DirectoryQuery { page: Some(2), scope_version: version, ..Default::default() }
                    .normalized()
                    .is_err()
            );
        }
        assert!(
            DirectoryQuery { page: Some(2), scope_version: Some("v1".into()), ..Default::default() }
                .normalized()
                .is_ok()
        );
        let ids = serde_json::from_value(serde_json::json!("a")).unwrap();
        assert!(
            DirectoryQuery { ids: Some(ids), q: Some("name".into()), ..Default::default() }
                .normalized()
                .is_err()
        );
    }
    #[test]
    fn content_and_authorization_changes_invalidate_the_directory_version() {
        let query = DirectoryQuery::default().normalized().unwrap();
        let original = DirectoryPage::from_snapshot(vec![item("a", "原名称")], &query, scope("v1")).unwrap();
        let renamed = DirectoryPage::from_snapshot(vec![item("a", "新名称")], &query, scope("v1")).unwrap();
        let mut disabled = item("a", "原名称");
        disabled.status = "disabled".into();
        let disabled = DirectoryPage::from_snapshot(vec![disabled], &query, scope("v1")).unwrap();
        let revoked = DirectoryPage::from_snapshot(vec![], &query, scope("v2")).unwrap();
        assert_ne!(original.scope_version, renamed.scope_version);
        assert_ne!(original.scope_version, disabled.scope_version);
        assert_ne!(original.scope_version, revoked.scope_version);
    }
    #[test]
    fn stable_paging_retains_empty_business_candidates_and_stopped_objects() {
        let query = DirectoryQuery { page_size: Some(1), ..Default::default() }.normalized().unwrap();
        let rows = vec![item("b", "same"), item("a", "same")];
        let first = DirectoryPage::from_snapshot(rows.clone(), &query, scope("v1")).unwrap();
        let second_query =
            DirectoryQuery { page: Some(2), scope_version: Some(first.scope_version.clone()), ..query }
                .normalized()
                .unwrap();
        let second = DirectoryPage::from_snapshot(rows, &second_query, scope("v1")).unwrap();
        assert_eq!(first.items[0].id, "a");
        assert_eq!(second.items[0].id, "b");
        assert_eq!(first.total, 2);
        assert_eq!(first.scope_version, second.scope_version);
    }
    #[test]
    fn invalid_sizes_selected_limit_and_snapshot_overflow_fail_closed() {
        for size in [0, 101] {
            assert!(DirectoryQuery { page_size: Some(size), ..Default::default() }.normalized().is_err());
        }
        let too_many = (0..101).map(|id| format!("id-{id}")).collect::<Vec<_>>().join(",");
        assert!(
            serde_json::from_value::<DirectorySelectedQuery>(serde_json::json!({"ids": too_many})).is_err()
        );
        assert!(
            DirectoryPage::from_snapshot(
                vec![item("a", "x"); DIRECTORY_LIMIT + 1],
                &DirectoryQuery::default(),
                scope("v1")
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<DirectorySelectedQuery>(serde_json::json!({"ids": "a", "q": "x"}))
                .is_err()
        );
    }
}
