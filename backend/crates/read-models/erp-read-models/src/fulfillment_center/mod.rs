//! 客户验收跨域工作台与登记结果；本域详情由履约服务持有。
mod acceptance_eligibility;
pub mod access;
pub mod dto;
pub mod repository;
use dto::CommitCustomerAcceptanceView;
use erp_core::ids::SalesOrderId;
use erp_fulfillment::service::FulfillmentService;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 组合销售版本展示信息与履约资格、验收历史的只读服务。
pub struct FulfillmentReadService {
    db: Database,
    domain: FulfillmentService,
}
impl FulfillmentReadService {
    /// 装配客户验收时，使用入口数据库构造只读组合，不持有审批或写事务能力。
    ///
    /// # 参数
    /// * `db` - 入口数据库
    ///
    /// # 返回
    /// 返回持有该数据库与履约领域服务的只读组合，构造不执行 I/O。
    pub fn new(db: Database) -> Self {
        Self { domain: FulfillmentService::new(db.clone()), db }
    }
    /// 生成客户验收视图时，读取命令收据所指向的验收单及剩余资格；根事务和回放均使用原查询配置。
    ///
    /// # 参数
    /// * `acceptance_id` - 已提交的客户验收单
    /// * `sales_order_id` - 剩余资格所依据的销售单
    /// * `executor` - 命令授权、回执和结果共用的原快照执行器。
    ///
    /// # 返回
    /// 返回验收单详情和该销售单的剩余验收资格。
    ///
    /// # 错误
    /// 非活动快照、结果销售来源不匹配、验收详情或剩余资格读取失败时拒绝。
    pub async fn committed_customer_acceptance_view(
        &self,
        acceptance_id: &str,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<CommitCustomerAcceptanceView> {
        let boundary = executor
            .boundary()
            .filter(|boundary| boundary.is_snapshot() && boundary.is_current(executor))
            .ok_or_else(|| {
                Error::ValidationError(
                    "暂时无法查看本次验收结果，请在销售单中核对验收进度，勿重复提交".into(),
                )
            })?;

        let acceptance = self.domain.load_customer_acceptance(acceptance_id, executor).await?.acceptance;
        if acceptance.sales_order_id != sales_order_id.as_ref() {
            return Err(Error::ConflictError(
                "验收结果与当前销售单不一致，请在原销售单中核对验收记录".into(),
            ));
        }

        let remaining_eligibility =
            self.load_acceptance_eligibility(sales_order_id.as_ref(), executor).await?;

        if !boundary.is_current(executor) {
            return Err(Error::ValidationError(
                "本次验收结果查询已失效，请刷新销售单后核对验收进度，勿重复提交".into(),
            ));
        }
        Ok(CommitCustomerAcceptanceView { acceptance, remaining_eligibility })
    }
}
