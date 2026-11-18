//! 退货、退款与冲正的详情和分页读模型。
mod approval;
mod customer_refund;
mod customer_refund_list;
pub mod dto;
mod payment_reversal;
mod purchase_return;
pub use purchase_return::PurchaseReturnListView;
mod receipt_reversal;
mod repository;
mod supplier_refund;
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_procurement::{FailClosedPurchaseDataScopePort, PurchaseDataScopePort};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 组合逆向本域事实与只读审批摘要的查询入口。
#[derive(Clone)]
pub struct ReturnsReadService {
    db: Database,
    authorization: Arc<AuthorizationService>,
    purchase_scope: Arc<dyn PurchaseDataScopePort>,
}
impl ReturnsReadService {
    // 核对退货中心边界时，已完成命令授权的投影只允许沿调用方当前快照继续读取。
    fn ensure_command_snapshot(executor: &mut dyn Executor) -> Result<()> {
        if executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            Ok(())
        } else {
            Err(Error::ValidationError(
                "暂时无法查看本次退款、退货或冲正结果，请打开原单据核对当前状态，勿重复操作".into(),
            ))
        }
    }

    /// 装配退货中心时，使用组合根提供的数据库读取退货与审批事实。
    ///
    /// # 参数
    /// * `authorization` - 组合根构造并共享的统一授权服务。
    ///
    /// # 返回
    /// 返回未注入采购范围 Port 的服务。
    ///
    /// # 关键业务约束
    /// 采购退货列表、详情必须改用 `with_purchase_scope`，未注入时失败关闭。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization, purchase_scope: FailClosedPurchaseDataScopePort::shared() }
    }

    /// 组装退货中心时填入组合层注入的采购范围 Port。
    ///
    /// # 参数
    /// * `purchase_scope` - 组合层注入的采购范围 Port
    ///
    /// # 返回
    /// 返回可解析采购退货范围的服务。
    ///
    /// # 关键业务约束
    /// 不得把 RBAC 直接交给退货域解释原始范围。
    pub fn with_purchase_scope(mut self, purchase_scope: Arc<dyn PurchaseDataScopePort>) -> Self {
        self.purchase_scope = purchase_scope;
        self
    }

    // 处理退货中心时，生成绑定当前采购 Port 的访问器。
    //
    // 返回无授权缓存的采购访问器。
    //
    // 业务边界：
    // 采购退货必须沿来源采购单责任接入。
    fn purchase_access(&self) -> crate::purchase_center::access::PurchaseAccess {
        crate::purchase_center::access::PurchaseAccess::new(self.db.clone(), Arc::clone(&self.purchase_scope))
    }
}

#[cfg(test)]
mod command_snapshot_tests {
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::{NoTransaction, ReadExecutor};

    use super::*;

    // 验证退货中心：裸投影不能在提交后换用普通执行器，并且在读取数据库之前失败。
    #[tokio::test]
    async fn command_views_reject_non_snapshot_executors_before_loading_records() {
        let client =
            mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1").await.unwrap();
        let service =
            ReturnsReadService::new(client.database("return_view_boundary"), unavailable_authorization());
        let errors = [
            service.load_customer_refund_view("id", &mut NoTransaction).await.unwrap_err(),
            service.load_supplier_refund_view("id", &mut ReadExecutor::default()).await.unwrap_err(),
            service.load_receipt_reversal_view("id", &mut NoTransaction).await.unwrap_err(),
            service.load_payment_reversal_view("id", &mut ReadExecutor::default()).await.unwrap_err(),
        ];
        for error in errors {
            assert!(
                matches!(error, Error::ValidationError(reason) if reason == "暂时无法查看本次退款、退货或冲正结果，请打开原单据核对当前状态，勿重复操作")
            );
        }
    }
}
