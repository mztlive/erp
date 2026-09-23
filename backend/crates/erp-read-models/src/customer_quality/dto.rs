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

impl CurrentQualityRow {
    /// 构造当前口径行。
    ///
    /// # 参数
    /// * `row_id` - 行稳定身份
    /// * `kind` - 行类型
    /// * `gross_total` - 含税总额
    ///
    /// # 返回
    /// 返回订单数为零、可选字段全空的行。
    ///
    /// # 错误
    /// 无。
    pub fn new(row_id: String, kind: String, gross_total: String) -> Self {
        Self {
            row_id,
            kind,
            customer_id: None,
            customer_no: None,
            customer_name: None,
            group_id: None,
            label: None,
            owner_user_id: None,
            owner_user_name: None,
            owner_org_unit_id: None,
            owner_org_unit_name: None,
            customer_count: None,
            order_count: 0,
            gross_total,
            unpriced_count: 0,
            first_effective_at: None,
            latest_effective_at: None,
        }
    }

    /// 设置客户身份。
    ///
    /// # 参数
    /// * `customer_id` - 客户身份
    /// * `customer_no` - 客户编号
    /// * `customer_name` - 客户名称
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_customer(
        mut self,
        customer_id: Option<String>,
        customer_no: Option<String>,
        customer_name: Option<String>,
    ) -> Self {
        self.customer_id = customer_id;
        self.customer_no = customer_no;
        self.customer_name = customer_name;
        self
    }

    /// 设置现任归属。
    ///
    /// # 参数
    /// * `owner_user_id` - 现任负责人
    /// * `owner_user_name` - 负责人姓名
    /// * `owner_org_unit_id` - 现任组织
    /// * `owner_org_unit_name` - 组织名称
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_ownership(
        mut self,
        owner_user_id: Option<String>,
        owner_user_name: Option<String>,
        owner_org_unit_id: Option<String>,
        owner_org_unit_name: Option<String>,
    ) -> Self {
        self.owner_user_id = owner_user_id;
        self.owner_user_name = owner_user_name;
        self.owner_org_unit_id = owner_org_unit_id;
        self.owner_org_unit_name = owner_org_unit_name;
        self
    }

    /// 设置分组身份。
    ///
    /// # 参数
    /// * `group_id` - 分组身份
    /// * `label` - 分组展示名
    /// * `customer_count` - 分组客户数
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_group(
        mut self,
        group_id: Option<String>,
        label: Option<String>,
        customer_count: Option<usize>,
    ) -> Self {
        self.group_id = group_id;
        self.label = label;
        self.customer_count = customer_count;
        self
    }

    /// 设置订单计数与缺版本数。
    ///
    /// # 参数
    /// * `order_count` - 订单数
    /// * `unpriced_count` - 缺版本数
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_counts(mut self, order_count: usize, unpriced_count: usize) -> Self {
        self.order_count = order_count;
        self.unpriced_count = unpriced_count;
        self
    }

    /// 设置生效区间。
    ///
    /// # 参数
    /// * `first_effective_at` - 首次生效
    /// * `latest_effective_at` - 最近生效
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_effective_range(
        mut self,
        first_effective_at: Option<String>,
        latest_effective_at: Option<String>,
    ) -> Self {
        self.first_effective_at = first_effective_at;
        self.latest_effective_at = latest_effective_at;
        self
    }
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

impl HistoryQualityRow {
    /// 构造历史口径行。
    ///
    /// # 参数
    /// * `row_id` - 行稳定身份
    /// * `kind` - 行类型
    /// * `gross_total` - 含税总额
    ///
    /// # 返回
    /// 返回可选字段全空的行。
    ///
    /// # 错误
    /// 无。
    pub fn new(row_id: String, kind: String, gross_total: String) -> Self {
        Self {
            row_id,
            kind,
            group_id: None,
            label: None,
            attribution_user_id: None,
            attribution_user_name: None,
            attribution_org_unit_id: None,
            attribution_org_unit_name: None,
            attribution_path: None,
            order_id: None,
            order_no: None,
            customer_id: None,
            customer_name: None,
            effective_at: None,
            order_count: None,
            gross_total,
            unpriced_count: 0,
        }
    }

    /// 设置分组身份。
    ///
    /// # 参数
    /// * `group_id` - 分组身份
    /// * `label` - 分组展示名
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_group(mut self, group_id: Option<String>, label: Option<String>) -> Self {
        self.group_id = group_id;
        self.label = label;
        self
    }

    /// 设置冻结人员归属。
    ///
    /// # 参数
    /// * `user_id` - 冻结人员
    /// * `user_name` - 人员姓名
    /// * `org_unit_id` - 冻结组织
    /// * `org_unit_name` - 组织名称
    /// * `path` - 组织祖先路径
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
        path: Option<Vec<String>>,
    ) -> Self {
        self.attribution_user_id = user_id;
        self.attribution_user_name = user_name;
        self.attribution_org_unit_id = org_unit_id;
        self.attribution_org_unit_name = org_unit_name;
        self.attribution_path = path;
        self
    }

    /// 设置归属订单。
    ///
    /// # 参数
    /// * `order_id` - 订单身份
    /// * `order_no` - 订单号
    /// * `customer_id` - 客户身份
    /// * `customer_name` - 客户名称
    /// * `effective_at` - 生效时间
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_order(
        mut self,
        order_id: Option<String>,
        order_no: Option<String>,
        customer_id: Option<String>,
        customer_name: Option<String>,
        effective_at: Option<String>,
    ) -> Self {
        self.order_id = order_id;
        self.order_no = order_no;
        self.customer_id = customer_id;
        self.customer_name = customer_name;
        self.effective_at = effective_at;
        self
    }

    /// 设置订单计数与缺版本数。
    ///
    /// # 参数
    /// * `order_count` - 订单数
    /// * `unpriced_count` - 缺版本数
    ///
    /// # 返回
    /// 返回更新后的行。
    ///
    /// # 错误
    /// 无。
    pub fn with_counts(mut self, order_count: Option<usize>, unpriced_count: usize) -> Self {
        self.order_count = order_count;
        self.unpriced_count = unpriced_count;
        self
    }
}

/// 分页行集合；排序与分页最后执行，导出保留全部匹配行。
#[derive(Debug, Serialize)]
pub struct QualityRows<T> {
    pub dimension: String,
    pub items: Vec<T>,
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::{CurrentQualityRow, HistoryQualityRow, QualityScope};

    #[test]
    fn current_row_builder_preserves_mandatory_fields() {
        let row =
            CurrentQualityRow::new("customer:c-1".to_string(), "customer".to_string(), "100.00".to_string())
                .with_counts(2, 1);
        assert_eq!(row.row_id, "customer:c-1");
        assert_eq!(row.kind, "customer");
        assert_eq!(row.gross_total, "100.00");
        assert_eq!(row.order_count, 2);
        assert_eq!(row.unpriced_count, 1);
    }

    #[test]
    fn current_row_optional_builders_do_not_touch_mandatory_fields() {
        let row = CurrentQualityRow::new(
            "owner_user:u-1".to_string(),
            "owner_user".to_string(),
            "0.00".to_string(),
        )
        .with_group(Some("u-1".to_string()), Some("销售 · u-1".to_string()), Some(3))
        .with_ownership(Some("u-1".to_string()), Some("销售".to_string()), None, None)
        .with_effective_range(Some("2026-09-01".to_string()), None);
        assert_eq!(row.group_id.as_deref(), Some("u-1"));
        assert_eq!(row.owner_user_id.as_deref(), Some("u-1"));
        assert_eq!(row.first_effective_at.as_deref(), Some("2026-09-01"));
        assert!(row.customer_id.is_none());
    }

    #[test]
    fn history_row_builder_preserves_mandatory_fields() {
        let row =
            HistoryQualityRow::new("order:o-1".to_string(), "sales_order".to_string(), "50.00".to_string())
                .with_order(
                    Some("o-1".to_string()),
                    Some("SO-1".to_string()),
                    Some("c-1".to_string()),
                    Some("测试客户".to_string()),
                    Some("2026-09-01".to_string()),
                )
                .with_counts(Some(1), 0);
        assert_eq!(row.row_id, "order:o-1");
        assert_eq!(row.order_no.as_deref(), Some("SO-1"));
        assert_eq!(row.order_count, Some(1));
    }

    #[test]
    fn scope_builder_preserves_mandatory_fields() {
        let scope = QualityScope::new("authorized".to_string(), "当前负责客户".to_string(), "v3".to_string())
            .with_label("历史负责订单".to_string());
        assert_eq!(scope.id, "authorized");
        assert_eq!(scope.label, "历史负责订单");
        assert_eq!(scope.permission_version, "v3");
    }
}

/// 响应范围摘要；不暴露完整角色证明或人员集合。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityScope {
    pub id: String,
    pub label: String,
    pub permission_version: String,
}

impl QualityScope {
    /// 构造响应范围摘要。
    ///
    /// # 参数
    /// * `id` - 范围身份
    /// * `label` - 范围展示名
    /// * `permission_version` - 权限版本
    ///
    /// # 返回
    /// 返回范围摘要。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: String, label: String, permission_version: String) -> Self {
        Self { id, label, permission_version }
    }

    /// 设置范围展示名。
    ///
    /// # 参数
    /// * `label` - 范围展示名
    ///
    /// # 返回
    /// 返回更新后的摘要。
    ///
    /// # 错误
    /// 无。
    pub fn with_label(mut self, label: String) -> Self {
        self.label = label;
        self
    }
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
