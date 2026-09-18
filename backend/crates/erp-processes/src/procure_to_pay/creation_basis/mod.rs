//! 采购创建依据与按销售当前版本剩余数量建单。
//!
//! 创建依据由销售单当前版本的 `GOODS_SERVICE` 行、当前采购覆盖数量和供应商
//! 当前合格供给共同形成。依据精确到销售当前版本、供应商、采购类型、付款条件与
//! 履约责任；一次依据命令只创建一张采购单，并在同一事务内提交审批。
//!
//! 供给、当前修订、可供投影与供应商结算事实由
//! `erp_read_models::purchase_center::repository::load_creation_basis_facts` 一次批量加载；拆单
//! 维度、稳定身份、产品类型映射、履约选项、成本选择、最大可创建数量与请求行
//! 规范化由 `erp_procurement::entity::purchase_order::creation_basis` 领域值对象承担。组合流程
//! 负责当前指针解析、任务归属与 RBAC、合格性筛选（条款有效期、AVAILABLE、
//! 零库存、每供应商稳定选一条）、事务编排与 View 映射。

mod create;
pub(super) mod supplier;
pub use create::{CreateBasisCommand, VerifiedBasisInput, persist_basis_draft};
pub use erp_procurement::service::purchase_order::creation_basis::validate_requested_quantities;
pub use erp_read_models::purchase_center::repository::{
    basis_groups_and_facts, basis_groups_for_order, load_effective_sales_order, stock_basis_groups_for_order,
};
/// 保持跨域命令原冲突类别。
pub fn procurement_quantity_changed() -> crate::Error {
    erp_procurement::service::purchase_order::creation_basis::procurement_quantity_changed().into()
}
