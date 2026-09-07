//! 履约本域查询、草稿准备与事务内事实写入。

pub mod acceptance_eligibility;
pub mod customer_acceptance;
pub mod customer_acceptance_lines;
pub mod customer_acceptance_posting;
pub mod delivery;
pub mod delivery_lines;
pub mod delivery_posting;
pub mod document_number;
pub mod electronic_delivery;
pub mod electronic_delivery_crypto;
pub mod purchase_receipt;
pub mod purchase_receipt_lines;
pub mod purchase_receipt_posting;
pub mod service_fulfillment;
pub mod service_fulfillment_confirm;
pub mod service_fulfillment_crypto;

use mongodb::Database;

/// 履约本域服务；跨域根事务与身份配置由履约流程持有。
pub struct FulfillmentService {
    pub(super) db: Database,
}

impl FulfillmentService {
    /// 使用数据库构造本域查询与事务内写入服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
