//! 结算主体目录响应；内容版本独立于权限及组织版本。
use application_core::directory::{DirectoryItem, DirectoryPage, DirectoryQuery, DirectoryScope};
use erp_core::common::time::Instant;
use serde::Serialize;

use crate::{Error, Result};

/// 后台公共候选目录，不携带数据范围拒绝或权限版本。
#[derive(Serialize)]
pub struct SettlementPartyDirectoryPage {
    pub items: Vec<DirectoryItem>,
    pub total: usize,
    pub page: u64,
    pub page_size: u32,
    pub scope_version: String,
    pub as_of: String,
}

impl SettlementPartyDirectoryPage {
    /// 按目录内容生成稳定分页及版本。
    /// # 参数
    /// `items` 为当前我方公司轻量快照，`query` 为已规范化的请求。
    /// # 返回
    /// 搜索或回显结果；空结果表示没有匹配主体。
    /// # 错误
    /// 目录超限或请求的内容版本过期时拒绝。
    pub(crate) fn from_snapshot(items: Vec<DirectoryItem>, query: &DirectoryQuery) -> Result<Self> {
        let page = DirectoryPage::from_snapshot(
            items,
            query,
            DirectoryScope {
                ids: None,
                scope_version: "settlement-party-directory.v1".into(),
                policy_version: 0,
                organization_version: 0,
                as_of: Instant::now().as_utc().to_rfc3339(),
                no_scope: false,
            },
        )?;
        if query.scope_version.as_ref().is_some_and(|version| version != &page.scope_version) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：目录已变化，请重新查询".into()));
        }
        Ok(Self {
            items: page.items,
            total: page.total,
            page: page.page,
            page_size: page.page_size,
            scope_version: page.scope_version,
            as_of: page.as_of,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> DirectoryItem {
        DirectoryItem { id: id.into(), code: id.into(), name: id.into(), status: "active".into() }
    }

    #[test]
    fn empty_directory_has_no_scope_denial_or_authorization_versions() {
        let page = SettlementPartyDirectoryPage::from_snapshot(vec![], &DirectoryQuery::default()).unwrap();
        let json = serde_json::to_value(page).unwrap();
        assert_eq!(json["items"], serde_json::json!([]));
        assert_eq!(json["total"], 0);
        for field in ["empty_reason", "policy_version", "organization_version"] {
            assert!(json.get(field).is_none(), "{field}");
        }
    }

    #[test]
    fn stable_pages_and_selected_identity_preserve_content_version() {
        let items = vec![item("b"), item("a")];
        let query = DirectoryQuery { page_size: Some(1), ..Default::default() }.normalized().unwrap();
        let first = SettlementPartyDirectoryPage::from_snapshot(items.clone(), &query).unwrap();
        let next =
            DirectoryQuery { page: Some(2), scope_version: Some(first.scope_version.clone()), ..query };
        let second = SettlementPartyDirectoryPage::from_snapshot(items.clone(), &next).unwrap();
        assert_eq!(first.items[0].id, "a");
        assert_eq!(second.items[0].id, "b");
        assert_eq!(first.scope_version, second.scope_version);
        assert_eq!(second.total, 2);
        assert!(SettlementPartyDirectoryPage::from_snapshot(vec![item("a")], &next).is_err());
        let selected = DirectoryQuery {
            ids: Some(serde_json::from_value(serde_json::json!("a,b")).unwrap()),
            ..Default::default()
        };
        let page = SettlementPartyDirectoryPage::from_snapshot(items, &selected).unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.page_size, 100);
        assert_eq!(page.scope_version, first.scope_version);
    }
}
