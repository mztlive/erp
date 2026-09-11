//! W16 响应合同；金额为十进制字符串，不可用浮点数累计。
use serde::{Deserialize, Serialize};

/// 全部查询及导出复用的筛选，不接收客户端提供的权限集合。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfitLossQuery {
    pub from: String,
    pub to: String,
    pub period_basis: String,
    #[serde(default)]
    pub scope_id: String,
    #[serde(default = "default_coverage")]
    pub coverage: String,
    pub customer_id: Option<String>,
    pub sales_order_id: Option<String>,
    pub benefit_scenario: Option<String>,
    pub cost_types: Option<String>,
    #[serde(default = "default_dimension")]
    pub dimension: String,
    pub q: Option<String>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}
/// 默认成本完整筛选。
fn default_coverage() -> String {
    "covered".into()
}
/// 默认销售单维度。
fn default_dimension() -> String {
    "sales_order".into()
}
/// 默认盈亏升序。
fn default_sort() -> String {
    "actualProfitLossNet:asc".into()
}
/// 默认页码。
fn default_page() -> usize {
    1
}
/// 默认页大小。
fn default_page_size() -> usize {
    20
}

/// 显式可选期间口径，未设置全局默认。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodBasisConfig {
    pub allowed_period_bases: Vec<PeriodBasisOption>,
    pub configuration_version: String,
}
#[derive(Debug, Serialize)]
pub struct PeriodBasisOption {
    pub code: String,
    pub label: String,
    pub explanation: String,
}

/// 已校验金额；缺成本时利润字段不返回。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub net_sales_revenue: String,
    pub actual_procurement_cost_net: String,
    pub actual_fulfillment_cost_net: String,
    pub reductions_net: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_profit_loss_net: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossRow {
    pub row_id: String,
    pub object_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    pub identity_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_label: Option<String>,
    pub benefit_scenarios: Vec<String>,
    pub fulfillment_modes: Vec<String>,
    #[serde(flatten)]
    pub totals: Totals,
    pub coverage_state: String,
    pub coverage_blockers: Vec<CoverageBlocker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_cost_occurred_at: Option<String>,
    pub allowed_drilldowns: Vec<String>,
    pub cost_entry_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CoverageBlocker {
    pub code: String,
    pub message: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub id: String,
    pub label: String,
    pub permission_version: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub from: String,
    pub to: String,
    pub basis: String,
    pub basis_label: String,
    pub timezone: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Freshness {
    pub projected_at: String,
    pub source_watermark: String,
    pub state: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub covered_net_revenue: String,
    pub uncovered_net_revenue: String,
    pub coverage_rate: String,
    pub reliability: String,
    pub coverage_state: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldPermissions {
    pub can_view_revenue: bool,
    pub can_view_cost: bool,
    pub can_view_profit: bool,
    pub can_export: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub period: String,
    pub net_sales_revenue: String,
    pub actual_cost_net: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_profit_loss_net: Option<String>,
    pub reliability: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostComposition {
    pub cost_type: String,
    pub label: String,
    pub net_amount: String,
    pub share: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageReference {
    pub stage: String,
    pub label: String,
    pub procurement_cost_net: String,
    pub fulfillment_cost_net: String,
    pub total_net: String,
    pub note: String,
}
#[derive(Debug, Serialize)]
pub struct Rows {
    pub dimension: String,
    pub items: Vec<ProfitLossRow>,
    pub total: usize,
}
/// 一次一致快照产生的分析视图。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossView {
    pub scope: Scope,
    pub period: Period,
    pub business_type: String,
    pub amount_basis: String,
    pub amount_basis_label: String,
    pub business_type_label: String,
    pub formula_version: String,
    pub formula_text: String,
    pub freshness: Freshness,
    pub coverage: Coverage,
    pub totals: Totals,
    pub field_permissions: FieldPermissions,
    pub trend: Vec<TrendPoint>,
    pub cost_composition: Vec<CostComposition>,
    pub stage_reference: Vec<StageReference>,
    pub rows: Rows,
    pub filter_summary: String,
    pub excluded_note: String,
}
/// 同步完成的全量筛选导出；文件内容由服务端生成，不伪造后台任务。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossExport {
    pub csv_content: String,
    pub file_name: String,
    pub row_count: usize,
    pub generated_at: String,
}
