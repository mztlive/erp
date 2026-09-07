//! 履约创建、确认、过账及任务的跨域事务组合。

pub mod customer_acceptance;
mod delivery;
mod delivery_posting;
mod electronic_delivery;
mod purchase_context;
mod purchase_receipt;
mod purchase_receipt_posting;
mod service_confirm;
pub(crate) mod service_crypto;
mod service_fulfillment;
pub mod task;

use std::sync::Arc;

use erp_fulfillment::service::FulfillmentService;
use erp_identity::SharedRbacService;
use erp_party::SensitiveDataCodec;
use mongodb::Database;

/// 保留入口提供的密钥、编解码器和对象读取能力，持有完整履约根事务。
pub struct FulfillmentProcess {
    pub(super) db: Database,
    pub(super) fingerprint_key: Vec<u8>,
    pub(super) sensitive_data: Arc<SensitiveDataCodec>,
    pub(super) rbac: SharedRbacService,
    pub(super) object_read: Arc<dyn erp_workflow::ApprovalObjectReadPort>,
}

impl FulfillmentProcess {
    /// 使用原入口配置构造流程；指纹密钥和敏感信息编解码器保持启动期共享。
    pub fn new(db: Database, fingerprint_key: Vec<u8>, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        let rbac = crate::adapters::identity::shared_rbac_service(db.clone());
        Self {
            db,
            fingerprint_key,
            sensitive_data,
            rbac,
            object_read: Arc::new(erp_workflow::FailClosedObjectReadPort),
        }
    }

    /// 注入组合根已经配置的审批对象读取能力。
    pub fn with_object_read(mut self, object_read: Arc<dyn erp_workflow::ApprovalObjectReadPort>) -> Self {
        self.object_read = object_read;
        self
    }

    pub(super) fn domain(&self) -> FulfillmentService {
        FulfillmentService::new(self.db.clone())
    }
}
