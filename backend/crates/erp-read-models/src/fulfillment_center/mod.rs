//! 客户验收跨域工作台与登记结果；本域详情由履约服务持有。
mod acceptance_eligibility;
pub mod dto;
pub mod repository;
use crate::Result;
use dto::CommitCustomerAcceptanceView;
use erp_core::ids::SalesOrderId;
use erp_fulfillment::service::FulfillmentService;
use mongodb::Database;

/// 组合销售版本展示信息与履约资格、验收历史的只读服务。
pub struct FulfillmentReadService {
    db: Database,
    domain: FulfillmentService,
}
impl FulfillmentReadService {
    /// 使用入口数据库构造只读组合，不持有审批或写事务能力。
    pub fn new(db: Database) -> Self {
        Self {
            domain: FulfillmentService::new(db.clone()),
            db,
        }
    }
    /// 读取命令收据所指向的验收单及剩余资格；根事务和回放均使用原查询配置。
    pub async fn committed_customer_acceptance_view(
        &self,
        acceptance_id: &str,
        sales_order_id: &SalesOrderId,
    ) -> Result<CommitCustomerAcceptanceView> {
        let acceptance = self
            .domain
            .customer_acceptance_detail(acceptance_id)
            .await?
            .acceptance;
        let remaining_eligibility = self.acceptance_eligibility(sales_order_id.as_ref()).await?;
        Ok(CommitCustomerAcceptanceView {
            acceptance,
            remaining_eligibility,
        })
    }
}
