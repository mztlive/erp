//! 客户对象中心的关联业务与应收读模型用例。
//!
//! 页面只接收最近摘要和跨页指标；Service 不返回可诱导客户端继续拉全量的分页游标。

use database::{CustomerExt, NoTransaction, ReceivableExt};
use entities::{
    common::time::{BusinessDate, Instant},
    contract::ContractStatus,
    money::Amount,
    sales_order::{CloseStatus, CommercialStatus},
};
use mongodb::Database;
use serde::Serialize;

use crate::errors::{Error, Result};

const RECENT_RELATED_LIMIT: u32 = 5;

/// 客户中心最近合同摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerCenterContractView {
    pub id: String,
    pub contract_no: String,
    pub status: ContractStatus,
}

/// 客户中心最近销售单摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerCenterSalesOrderView {
    pub id: String,
    pub order_no: String,
    pub commercial_status: CommercialStatus,
    pub close_status: CloseStatus,
    pub created_at: u64,
}

/// 客户中心合同与销售单读模型。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerCenterRelatedView {
    pub active_contract_count: i64,
    pub in_progress_sales_order_count: i64,
    pub contracts: Vec<CustomerCenterContractView>,
    pub sales_orders: Vec<CustomerCenterSalesOrderView>,
    pub projected_at: Instant,
}

/// 客户中心应收跨账户汇总。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerCenterReceivableView {
    pub receivable_balance: Amount,
    pub overdue_amount: Amount,
    pub open_invoiceable_total: Amount,
    pub earliest_overdue_date: Option<BusinessDate>,
    pub projected_at: Instant,
}

/// 客户对象中心只读用例服务。
pub struct CustomerCenterReadService {
    db: Database,
}

impl CustomerCenterReadService {
    /// 创建客户对象中心读服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 查询合同/销售单最近摘要与跨页指标。
    ///
    /// # 错误
    /// 客户不存在或聚合读取失败时返回错误。
    pub async fn related(&self, customer_id: &str) -> Result<CustomerCenterRelatedView> {
        let row = self
            .db
            .customer_accounts()
            .customer_center_related(customer_id, RECENT_RELATED_LIMIT, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
        Ok(CustomerCenterRelatedView {
            active_contract_count: row.active_contract_count,
            in_progress_sales_order_count: row.in_progress_sales_order_count,
            contracts: row
                .contracts
                .into_iter()
                .map(|contract| CustomerCenterContractView {
                    id: contract.id,
                    contract_no: contract.contract_no,
                    status: contract.status,
                })
                .collect(),
            sales_orders: row
                .sales_orders
                .into_iter()
                .map(|order| CustomerCenterSalesOrderView {
                    id: order.id,
                    order_no: order.order_no,
                    commercial_status: order.commercial_status,
                    close_status: order.close_status,
                    created_at: order.created_at,
                })
                .collect(),
            projected_at: Instant::now(),
        })
    }

    /// 查询跨应收账户的余额、逾期和可开票汇总。
    ///
    /// # 错误
    /// 客户不存在或聚合读取失败时返回错误。
    pub async fn receivable(&self, customer_id: &str) -> Result<CustomerCenterReceivableView> {
        self.ensure_customer_exists(customer_id).await?;
        let row = self
            .db
            .receivable_accounts()
            .customer_center_receivable(customer_id, BusinessDate::today(), &mut NoTransaction)
            .await?;
        Ok(CustomerCenterReceivableView {
            receivable_balance: row.receivable_balance,
            overdue_amount: row.overdue_amount,
            open_invoiceable_total: row.open_invoiceable_total,
            earliest_overdue_date: row.earliest_overdue_date,
            projected_at: Instant::now(),
        })
    }

    async fn ensure_customer_exists(&self, customer_id: &str) -> Result<()> {
        if self
            .db
            .customer_accounts()
            .find_customer(customer_id, &mut NoTransaction)
            .await?
            .is_some()
        {
            return Ok(());
        }
        Err(Error::NotFound("客户不存在".to_string()))
    }
}
