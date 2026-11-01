//! 客户对象中心的关联业务与应收读模型用例。
//!
//! 页面只接收最近摘要和跨页指标；Service 不返回可诱导客户端继续拉全量的分页游标。

use application_core::AuditActor;
use erp_contract::{ContractAccess, ContractStatus};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::money::Amount;
use erp_customer::CustomerAccess;
use erp_identity::SharedRbacService;
use erp_sales::entity::sales_order::{CloseStatus, CommercialStatus};
use mongodb::Database;
use persistence_core::Transactional;
use serde::Serialize;

use super::repository::CustomerCenterRelatedRow;
use crate::finance::funds_scope::FundsAccess;
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

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
#[derive(Clone)]
pub struct CustomerCenterReadService {
    db: Database,
    rbac: SharedRbacService,
    contracts: ContractAccess,
    customers: CustomerAccess,
    funds: FundsAccess,
}

impl CustomerCenterReadService {
    /// 创建客户对象中心读服务。
    ///
    /// # 参数
    /// * `db` - 应用数据库
    /// * `rbac` - 当前账号功能与销售范围授权服务
    /// * `contracts` - 合同独立责任范围访问器
    /// * `customers` - 客户资料范围访问器
    /// * `funds` - 票款来源范围访问器
    ///
    /// # 返回
    /// 返回绑定该数据库的只读服务，构造不访问数据库。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        rbac: SharedRbacService,
        contracts: ContractAccess,
        customers: CustomerAccess,
        funds: FundsAccess,
    ) -> Self {
        Self { db, rbac, contracts, customers, funds }
    }

    /// 查询合同/销售单最近摘要与跨页指标。
    ///
    /// # 参数
    /// * `customer_id` - 客户 ID
    /// * `actor` - 同一读取事务内重验客户、合同和销售范围的操作人
    ///
    /// # 返回
    /// 返回该客户的最近合同、销售单摘要和跨页计数。
    ///
    /// # 错误
    /// 客户不存在或聚合读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 在同一读取事务内重验客户对象 `customer:detail`、合同 `contract:list`、
    /// 销售 `sales_order:list` 三项；授权条件在关联查询的计数和分页前执行。
    pub async fn related(&self, customer_id: &str, actor: &AuditActor) -> Result<CustomerCenterRelatedView> {
        let this = self.clone();
        let actor = actor.clone();
        let customer_id = customer_id.to_string();
        let row = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.customers.require_with(actor.clone(), "detail", &customer_id, executor).await?;
                    let (contract_context, contracts) =
                        this.contracts.resolve(&actor, "list", executor).await?;
                    let (_, sales) = SalesAccess::new(this.db.clone(), this.rbac.clone())
                        .resolve(&actor, "list", &[], executor)
                        .await?;
                    if !contract_context.has_scope_rules() || sales.is_empty() {
                        return Err(Error::Forbidden("当前没有可查看的合同或销售业务范围".into()));
                    }
                    super::repository::CustomerCenterRepository::new(&this.db)
                        .customer_center_related(
                            &customer_id,
                            RECENT_RELATED_LIMIT,
                            &contracts.document(),
                            &sales.document(),
                            executor,
                        )
                        .await?
                        .ok_or_else(|| Error::NotFound("客户不存在".into()))
                })
            })
            .await?;
        Ok(row.into())
    }

    /// 查询跨应收账户的余额、逾期和可开票汇总。
    ///
    /// # 参数
    /// * `customer_id` - 客户 ID
    /// * `actor` - 同一读取事务内重验客户及应收来源范围的操作人
    ///
    /// # 返回
    /// 返回跨应收账户的余额、逾期金额、可开票合计和最早逾期日。
    ///
    /// # 错误
    /// 客户不存在或聚合读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 客户对象 `customer:detail` 与应收 `receivable_account:list` 缺一不可；
    /// 只汇总 FundsAccess 证明属于允许来源的应收子账。
    pub async fn receivable(
        &self,
        customer_id: &str,
        actor: &AuditActor,
    ) -> Result<CustomerCenterReceivableView> {
        let this = self.clone();
        let actor = actor.clone();
        let customer_id = customer_id.to_string();
        let row = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.customers.require_with(actor.clone(), "detail", &customer_id, executor).await?;
                    this.funds.customer_receivable_summary(&customer_id, &actor, executor).await
                })
            })
            .await?;
        Ok(CustomerCenterReceivableView {
            receivable_balance: row.receivable_balance,
            overdue_amount: row.overdue_amount,
            open_invoiceable_total: row.open_invoiceable_total,
            earliest_overdue_date: row.earliest_overdue_date,
            projected_at: Instant::now(),
        })
    }
}

impl From<CustomerCenterRelatedRow> for CustomerCenterRelatedView {
    fn from(row: CustomerCenterRelatedRow) -> Self {
        Self {
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
        }
    }
}
