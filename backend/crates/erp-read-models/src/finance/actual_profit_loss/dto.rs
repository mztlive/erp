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
    pub scope_version: Option<String>,
    #[serde(default = "default_coverage")]
    pub coverage: String,
    pub customer_id: Option<String>,
    pub sales_order_id: Option<String>,
    pub benefit_scenario: Option<String>,
    pub cost_types: Option<String>,
    pub attribution_user_ids: Option<application_core::QueryIds>,
    pub attribution_org_unit_ids: Option<application_core::QueryIds>,
    /// 精确历史分组下钻，取分组行 ID；空身份后缀表示未知归属。
    pub attribution_group: Option<String>,
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
    pub attribution_user_id: Option<String>,
    pub attribution_user_name: Option<String>,
    pub attribution_org_unit_id: Option<String>,
    pub attribution_org_unit_name: Option<String>,
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

impl ProfitLossRow {
    /// 构造盈亏行。
    ///
    /// # 参数
    /// * `row_id` - 行稳定身份
    /// * `object_type` - 对象类型
    /// * `identity_label` - 身份展示名
    /// * `coverage_state` - 覆盖状态
    ///
    /// # 返回
    /// 返回归属、金额、阻断全空的行。
    ///
    /// # 错误
    /// 无。
    pub fn new(row_id: String, object_type: String, identity_label: String, coverage_state: String) -> Self {
        Self {
            row_id,
            object_type,
            object_id: None,
            identity_label,
            customer_id: None,
            customer_label: None,
            attribution_user_id: None,
            attribution_user_name: None,
            attribution_org_unit_id: None,
            attribution_org_unit_name: None,
            benefit_scenarios: Vec::new(),
            fulfillment_modes: Vec::new(),
            totals: Totals::default(),
            coverage_state,
            coverage_blockers: Vec::new(),
            latest_cost_occurred_at: None,
            allowed_drilldowns: Vec::new(),
            cost_entry_ids: Vec::new(),
        }
    }

    /// 设置对象身份。
    ///
    /// # 参数
    /// * `object_id` - 对象身份
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_object_id(mut self, object_id: Option<String>) -> Self {
        self.object_id = object_id;
        self
    }

    /// 设置客户身份。
    ///
    /// # 参数
    /// * `customer_id` - 客户身份
    /// * `customer_label` - 客户展示名
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_customer(mut self, customer_id: Option<String>, customer_label: Option<String>) -> Self {
        self.customer_id = customer_id;
        self.customer_label = customer_label;
        self
    }

    /// 设置冻结归属。
    ///
    /// # 参数
    /// * `user_id` - 冻结人员
    /// * `user_name` - 人员姓名
    /// * `org_unit_id` - 冻结组织
    /// * `org_unit_name` - 组织名称
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_attribution(
        mut self,
        user_id: Option<String>,
        user_name: Option<String>,
        org_unit_id: Option<String>,
        org_unit_name: Option<String>,
    ) -> Self {
        self.attribution_user_id = user_id;
        self.attribution_user_name = user_name;
        self.attribution_org_unit_id = org_unit_id;
        self.attribution_org_unit_name = org_unit_name;
        self
    }

    /// 设置业务场景与履约方式。
    ///
    /// # 参数
    /// * `benefit_scenarios` - 业务场景
    /// * `fulfillment_modes` - 履约方式
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_modes(mut self, benefit_scenarios: Vec<String>, fulfillment_modes: Vec<String>) -> Self {
        self.benefit_scenarios = benefit_scenarios;
        self.fulfillment_modes = fulfillment_modes;
        self
    }

    /// 设置金额汇总与覆盖阻断。
    ///
    /// # 参数
    /// * `totals` - 已校验金额汇总
    /// * `coverage_blockers` - 覆盖阻断
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_totals(mut self, totals: Totals, coverage_blockers: Vec<CoverageBlocker>) -> Self {
        self.totals = totals;
        self.coverage_blockers = coverage_blockers;
        self
    }

    /// 设置下钻与成本来源。
    ///
    /// # 参数
    /// * `latest_cost_occurred_at` - 最近成本发生时间
    /// * `allowed_drilldowns` - 允许下钻
    /// * `cost_entry_ids` - 成本来源
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_drilldown(
        mut self,
        latest_cost_occurred_at: Option<String>,
        allowed_drilldowns: Vec<String>,
        cost_entry_ids: Vec<String>,
    ) -> Self {
        self.latest_cost_occurred_at = latest_cost_occurred_at;
        self.allowed_drilldowns = allowed_drilldowns;
        self.cost_entry_ids = cost_entry_ids;
        self
    }
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
#[derive(Debug, Clone, Default, Serialize)]
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
    pub empty_reason: Option<String>,
    pub scope_summary: String,
    pub as_of: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub scope_version: String,
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
    pub ownership_basis: String,
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

#[cfg(test)]
mod tests {
    use super::{FieldPermissions, ProfitLossRow};

    #[test]
    fn field_permissions_default_denies_all_views() {
        let permissions = FieldPermissions::default();
        assert!(!permissions.can_view_revenue);
        assert!(!permissions.can_view_cost);
        assert!(!permissions.can_view_profit);
        assert!(!permissions.can_export);
    }

    #[test]
    fn profit_loss_row_builder_preserves_mandatory_fields() {
        let row = ProfitLossRow::new(
            "order-1".to_string(),
            "sales_order".to_string(),
            "SO-001".to_string(),
            "COVERED".to_string(),
        )
        .with_object_id(Some("order-1".to_string()))
        .with_customer(Some("c-1".to_string()), Some("测试客户".to_string()));
        assert_eq!(row.row_id, "order-1");
        assert_eq!(row.object_type, "sales_order");
        assert_eq!(row.identity_label, "SO-001");
        assert_eq!(row.coverage_state, "COVERED");
        assert_eq!(row.object_id.as_deref(), Some("order-1"));
        assert_eq!(row.customer_label.as_deref(), Some("测试客户"));
    }

    #[test]
    fn profit_loss_row_defaults_leave_optionals_empty() {
        let row = ProfitLossRow::new(
            "order-2".to_string(),
            "sales_order".to_string(),
            "SO-002".to_string(),
            String::new(),
        );
        assert!(row.object_id.is_none());
        assert!(row.coverage_blockers.is_empty());
        assert!(row.cost_entry_ids.is_empty());
    }
}
