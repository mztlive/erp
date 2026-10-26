//! 人工目录匹配命令，不修改公司主档或正式供给。
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchCatalogItemRequest {
    pub sku_id: String,
    pub expected_version: u64,
}
