//! 域 D18 `receivable` 服务编排（页面：W11 客户往来）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 发票创建必须在同一事务注册 `BusinessDocument` 并调用统一绑定端口；
//!   `NO_APPROVAL` 返回空绑定，不查询发布定义、不启动实例、不建任务；
//! - 客户回款创建必须在同一事务注册 `BusinessDocument` 并绑定发布定义；
//! - 跨集合资金/票款过账（§8.3 不变量）→
//!   `persistence_core::Transactional::with_transaction`，闭包内按稳定顺序锁定两侧，
//!   不执行外部 HTTP/文件 IO。
//! - 资金类入口（回款过账、发票登记、红冲）以业务唯一键
//!   （回款单号/规范化发票号码）与状态迁移构成去重机制，重复提交只产生一条
//!   正式事实。回款过账只能作为审批最终通过动作。
//!
//! 跨域只经各领域 `*Ext` 扩展 trait 调对方域 Repository：D13 `sales_orders()` 校验来源
//! 销售单存在；D18 拥有 `invoice` 实体与仓储，D19 经 `invoices()` 复用。

use erp_identity::SharedRbacService;
use mongodb::Database;

use crate::adapters::identity::shared_rbac_service;

mod account;
mod adapter;
mod cancel_approval;
mod customer_receipt;
mod customer_receipt_posting;

mod dto;
mod invoice;
mod invoice_posting;
pub(crate) mod invoice_request;

pub(crate) mod invoice_task;
mod red_invoice;
mod start_approval;

pub use self::adapter::customer_receipt_object_readable;
pub use self::customer_receipt::{cancel_customer_receipt_approval_apply, post_customer_receipt_apply};
/// 客户往来服务。
///
/// 提供应收台账、回款与销项发票的查询与过账编排。
pub struct ReceivableProcess {
    db: Database,
    read: erp_read_models::finance::receivable::ReceivableReadService,
    finance: erp_finance::service::receivable::ReceivableService,
    rbac: SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
}

impl ReceivableProcess {
    /// 创建客户往来服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        let rbac = shared_rbac_service(db.clone());
        Self {
            read: erp_read_models::finance::receivable::ReceivableReadService::new(db.clone()),
            finance: erp_finance::service::receivable::ReceivableService::new(db.clone()),
            db,
            rbac,
            object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort),
        }
    }

    /// Inject composition-root object-read for approval binding.
    pub fn with_object_read(
        mut self,
        object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        self.object_read = object_read;
        self
    }
}
