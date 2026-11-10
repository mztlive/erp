//! 履约创建、确认、过账及任务的跨域事务组合。

pub mod customer_acceptance;
mod delivery;
mod delivery_posting;
pub mod direct_delivery_draft;
pub use delivery_posting::post_observed_supplier_delivery;
mod delivery_addresses;
mod delivery_packing;
mod delivery_tracking;
mod electronic_confirm;
mod electronic_delivery;
mod fulfillment_actual_cost;
mod purchase_context;
pub use purchase_context::{ensure_po_fulfillable, ensure_prepay_gate};
mod purchase_receipt;
mod purchase_receipt_posting;
mod service_confirm;
mod service_confirm_audit;
pub(crate) mod service_crypto;
mod service_fulfillment;
pub mod task;

use std::sync::Arc;

use erp_fulfillment::service::FulfillmentService;
use erp_party::SensitiveDataCodec;
use mongodb::Database;
use task::FulfillmentIdentityFactory;

use crate::workflow::FulfillmentWorkflow;

/// 保留入口提供的密钥、编解码器和对象读取能力，持有完整履约根事务。
#[derive(Clone)]
pub struct FulfillmentProcess {
    workflow: Arc<dyn FulfillmentWorkflow>,
    pub(super) db: Database,
    pub(super) fingerprint_key: Vec<u8>,
    pub(super) sensitive_data: Arc<SensitiveDataCodec>,
    pub(super) task_identity: FulfillmentIdentityFactory,
}

impl FulfillmentProcess {
    /// 使用原入口配置构造流程；指纹密钥和敏感信息编解码器保持启动期共享。
    ///
    /// # 参数
    /// * `db` - 业务数据库。
    /// * `fingerprint_key` - 启动期共享的指纹密钥。
    /// * `sensitive_data` - 启动期共享的敏感信息编解码器。
    /// * `workflow` - 应用装配的工作流绑定、材料和正式任务能力。
    /// * `task_identity` - 每次任务资格核验独立创建身份的工厂。
    ///
    /// # 返回
    /// 返回对象读取默认失败关闭的履约流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        fingerprint_key: Vec<u8>,
        sensitive_data: Arc<SensitiveDataCodec>,
        task_identity: FulfillmentIdentityFactory,
        workflow: Arc<dyn FulfillmentWorkflow>,
    ) -> Self {
        Self { workflow, db, fingerprint_key, sensitive_data, task_identity }
    }

    /// 用本流程的数据库构造履约领域服务，不另开事务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定同一数据库的 `FulfillmentService`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn domain(&self) -> FulfillmentService {
        FulfillmentService::new(self.db.clone())
    }
}

pub mod address_arrangement;
