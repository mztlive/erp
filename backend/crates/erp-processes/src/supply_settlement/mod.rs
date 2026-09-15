//! 域 D33 `supplier_settlement` 的跨域事务编排。
//!
//! 事务边界由本流程建立：
//! - 创建结算草稿：结算单 + 全部明细同事务（本域
//!   `create_statement_with_items` 要求事务执行器，§6.20）；
//! - 结算确认（§8.4 第 6 条）：锁定结算单并重验差异处理结果、形成结算单应付
//!   （财务 `PayableRepository::create_payable_with_entry`，来源类型
//!   `SupplierSettlement`）并更新结算状态，同一事务完成；最终成本差额由财务
//!   provider 判定，缺少权威原成本链时保持原错误，不伪造 `CostEntry`。
//!
//! 结算单域规则与写入归 `erp-supply`；履约订单与明细存在性由同域仓储提供，
//! 应付账户与原始分录归财务 provider；正式复核任务与审计在本流程编排。
//!
//! 资金/状态机入口一律幂等：创建键为 `statement_no`，确认/提交复核/作废重复提交
//! 返回原结算单当前视图（不重复形成应付、不重复推进状态）；差异处理以版本 CAS
//! 防并发覆盖。
mod difference;
mod draft;
mod evidence;
mod review;
mod reviewers;
pub use reviewers::SettlementReviewerOption;
mod review_posting;
mod review_preparation;
mod shared;
mod source;
mod void;
use erp_read_models::supplier_center::settlement::dto::SettlementReviewDecisionResult;
use erp_supply::dto::supplier_settlement as dto;
use erp_supply::dto::supplier_settlement::*;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::shared::*;
use mongodb::Database;
use shared::*;
/// 供应商结算跨工作流、审计与财务的唯一命令入口。
pub struct SupplierSettlementProcess {
    db: Database,
}
impl SupplierSettlementProcess {
    /// 使用原数据库依赖构造结算流程。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    fn domain(&self) -> SupplierSettlementService {
        SupplierSettlementService::new(self.db.clone())
    }
}

use erp_supply::service::supplier_settlement::review::{
    SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID, SETTLEMENT_REVIEW_OWNER_ROLE,
};

#[cfg(test)]
mod repository;
#[cfg(test)]
mod tests;
