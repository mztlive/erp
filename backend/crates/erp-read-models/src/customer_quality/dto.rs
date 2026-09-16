//! 双口径查询与视图合同；金额为十进制字符串，不可用浮点数累计。
//!
//! 当前口径只接受现任条件（`owner_user_ids`／`org_unit_ids`），历史口径只接受
//! 冻结条件（`attribution_user_ids`／`attribution_org_unit_ids`）；两类条件混用
//! 必须被拒绝，不得静默忽略。

use serde::{Deserialize, Serialize};

/// 当前负责口径查询：现任主责、主责所属组织与业务筛选求交。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentQualityQuery {
    pub from: String,
    pub to: String,
    pub owner_user_ids: Option<application_core::QueryIds>,
    pub org_unit_ids: Option<application_core::QueryIds>,
    pub include_descendants: Option<bool>,
    pub customer_id: Option<String>,
    /// 精确分组下钻，取分组行 ID；`user:<id>` 或 `org:<id>`。
    pub owner_group: Option<String>,
    pub q: Option<String>,
    #[serde(default = "default_current_dimension")]
    pub dimension: String,
    #[serde(default = "default_sort")]
    pub sort: String,
    pub scope_version: Option<String>,
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}

/// 历史贡献口径查询：冻结归属、历史组织路径与业务筛选求交。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryQualityQuery {
    pub from: String,
    pub to: String,
    pub attribution_user_ids: Option<application_core::QueryIds>,
    pub attribution_org_unit_ids: Option<application_core::QueryIds>,
    /// 精确历史分组下钻，取分组行 ID；空身份后缀表示未知归属。
    pub attribution_group: Option<String>,
    pub customer_id: Option<String>,
    pub q: Option<String>,
    #[serde(default = "default_history_dimension")]
    pub dimension: String,
    #[serde(default = "default_sort")]
    pub sort: String,
    pub scope_version: Option<String>,
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}

/// 默认当前口径按客户逐行展示。
fn default_current_dimension() -> String {
    "customer".into()
}

/// 默认历史口径按冻结人员分组展示。
fn default_history_dimension() -> String {
    "attribution_user".into()
}

/// 默认按订单数降序，同值按稳定键排序。
fn default_sort() -> String {
    "orderCount:desc".into()
}

/// 默认页码。
fn default_page() -> usize {
    1
}

/// 默认页大小。
fn default_page_size() -> usize {
    20
}

/// 已校验金额汇总；缺正式版本订单不计入含税总额。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityTotals {
    pub object_count: usize,
    pub order_count: usize,
    pub gross_total: String,
    pub unpriced_count: usize,
}

/// 当前口径行：客户行或现任分组行，二者永不混排。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentQualityRow {
    pub row_id: String,
    /// `customer`、`owner_user`、`owner_org` 三者之一。
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_org_unit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_org_unit_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_count: Option<usize>,
    pub order_count: usize,
    pub gross_total: String,
    pub unpriced_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_effective_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_effective_at: Option<String>,
}

/// 历史口径行：冻结分组行或归属订单行，身份一律来自快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQualityRow {
    pub row_id: String,
    /// `attribution_user`、`attribution_org`、`sales_order` 三者之一。
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_user_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_org_unit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_org_unit_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution_path: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_count: Option<usize>,
    pub gross_total: String,
    pub unpriced_count: usize,
}

/// 分页行集合；排序与分页最后执行，导出保留全部匹配行。
#[derive(Debug, Serialize)]
pub struct QualityRows<T> {
    pub dimension: String,
    pub items: Vec<T>,
    pub total: usize,
}

/// 响应范围摘要；不暴露完整角色证明或人员集合。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityScope {
    pub id: String,
    pub label: String,
    pub permission_version: String,
}

/// 上海自然日期间；口径固定为销售单生效日。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityPeriod {
    pub from: String,
    pub to: String,
    pub basis: String,
    pub basis_label: String,
    pub timezone: String,
}

/// 当前负责口径视图：现任归属分组与汇总。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentQualityView {
    pub empty_reason: Option<String>,
    pub scope_summary: String,
    pub as_of: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub scope_version: String,
    pub scope: QualityScope,
    pub period: QualityPeriod,
    pub ownership_basis: String,
    pub totals: QualityTotals,
    pub rows: QualityRows<CurrentQualityRow>,
    pub filter_summary: String,
    pub owner_options: Vec<application_core::FilterOption>,
    pub org_options: Vec<application_core::FilterOption>,
    pub can_export: bool,
}

/// 历史贡献口径视图：冻结归属分组与汇总。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQualityView {
    pub empty_reason: Option<String>,
    pub scope_summary: String,
    pub as_of: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub scope_version: String,
    pub scope: QualityScope,
    pub period: QualityPeriod,
    pub ownership_basis: String,
    pub totals: QualityTotals,
    pub rows: QualityRows<HistoryQualityRow>,
    pub filter_summary: String,
    pub attribution_user_options: Vec<application_core::FilterOption>,
    pub attribution_org_options: Vec<application_core::FilterOption>,
    pub can_export: bool,
}

/// 同步生成的全量筛选 CSV；文件内容由服务端生成。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityExport {
    pub csv_content: String,
    pub file_name: String,
    pub row_count: usize,
    pub generated_at: String,
}
