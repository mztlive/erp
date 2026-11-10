use crate::entity::resource_access::GrantSource;

/// 授权来源管理页，next_cursor 为排他主键游标。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrantSourcePage {
    pub items: Vec<GrantSource>,
    pub next_cursor: Option<String>,
}
