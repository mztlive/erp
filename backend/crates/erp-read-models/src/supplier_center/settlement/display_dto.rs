//! 结算页面的跨域可读名称；所有原领域字段保持原 HTTP 形状。

use erp_supply::dto::supplier_settlement::{
    SettlementDifferenceEvidenceView, SupplierSettlementItemView, SupplierSettlementStatementListView,
    SupplierSettlementStatementView,
};
use serde::Serialize;

use super::dto::SupplierSettlementStatementDetailView;

/// 当前授权结算单的名称投影；内部身份仍用于命令与深链。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementStatementDisplayView {
    /// 结算领域事实，展平以保持协议字段。
    #[serde(flatten)]
    pub statement: SupplierSettlementStatementView,
    /// 供应商当前法定名称。
    pub supplier_name: Option<String>,
    /// 对账负责人姓名。
    pub prepared_by_name: Option<String>,
    /// 差异处理人姓名。
    pub difference_handler_name: Option<String>,
    /// 实际复核人姓名。
    pub reviewed_by_name: Option<String>,
    /// 已形成应付的来源业务单号，与复核决定合同一致。
    pub payable_no: Option<String>,
}

/// 结算明细关联的供应商子订单与商品名称。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementItemDisplayView {
    /// 原结算明细事实。
    #[serde(flatten)]
    pub item: SupplierSettlementItemView,
    /// ERP 供应商子订单业务单号。
    pub supplier_order_no: Option<String>,
    /// 供应商系统的订单号。
    pub external_order_no: Option<String>,
    /// 关联供给的当前公司 SKU 名称。
    pub product_name: Option<String>,
    /// 采购单身份；当前来源合同未提供此关联。
    pub purchase_order_id: Option<String>,
    /// 采购单业务单号；缺失关联保持为空。
    pub purchase_order_no: Option<String>,
}

/// 差异补证的记录人及可读材料标签。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementEvidenceDisplayView {
    /// 原不可变补证事实。
    #[serde(flatten)]
    pub evidence: SettlementDifferenceEvidenceView,
    /// 记录人姓名。
    pub provided_by_name: Option<String>,
    /// 材料列表标签；无权威名称的引用按原顺序标注材料序号。
    pub evidence_reference_labels: Vec<String>,
}

/// 保留分页、授权版本和统计的结算列表名称视图。
pub type SettlementStatementDisplayList = SupplierSettlementStatementListView<SettlementStatementDisplayView>;

/// 保留正式复核责任与服务端动作的结算详情名称视图。
pub type SettlementStatementDisplayDetail = SupplierSettlementStatementDetailView<
    SettlementStatementDisplayView,
    SettlementItemDisplayView,
    SettlementEvidenceDisplayView,
>;
