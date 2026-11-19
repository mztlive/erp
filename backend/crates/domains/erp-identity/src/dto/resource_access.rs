use crate::entity::resource_access::GrantSource;

/// 管理员按接收账号查看记录读取授权来源时返回的分页视图。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrantSourcePage {
    /// 本页最多 100 条授权来源，包含已终止记录。
    pub items: Vec<GrantSource>,
    /// 下一页使用的排他主键游标；没有后续页时为空。
    pub next_cursor: Option<String>,
}
