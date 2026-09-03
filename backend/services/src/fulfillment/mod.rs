//! 域 D16 `fulfillment` 服务编排（页面：W06 客户验收、W01 履约任务作业面）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 采购收货、发货创建必须在同一事务注册 `BusinessDocument` 并调用统一绑定端口；
//!   `NO_APPROVAL` 返回空绑定，不查询发布定义、不启动实例、不建任务；
//! - 单集合无跨步骤原子性要求的 CRUD 传入 `&mut NoTransaction`；
//! - 表头+行创建、状态迁移+审计、过账（§8.2 第 1/2/5 条跨集合原子性）使用
//!   `database::Transactional::with_transaction`。
//!
//! 跨域协作（P3-service-api §2：只调对方 Repository，不依赖对方 Service）：
//! - D15 `purchase_order`：采购单/生效版本/版本行/采购销售分配；§8.1.5
//!   `PREPAY` 门槛按 B-G7（P2 依赖）经 D19 `payable` Repository 重算
//!   有效已过账付款净核销金额；
//! - D13 `sales_order`：销售版本行（预占归属与验收工作台）；
//! - D17 `inventory`：余额/流水/预占（入库过账与仓发过账）。
//!
//! 过账去重：状态守卫（仅草稿可过账/确认）+ `stock_movement` 的
//! `(source_document_id, source_line_id, movement_type)` 唯一索引 + 验收
//! `Draft → Posted` 状态机三重防护，重复过账返回 409，不产生第二条正式事实。

use std::sync::Arc;

use mongodb::Database;

use crate::iam::{self, SharedRbacService};
use crate::party::SensitiveDataCodec;

mod acceptance_eligibility;
mod customer_acceptance;
mod customer_acceptance_lines;
mod customer_acceptance_posting;
mod customer_acceptance_task;
mod delivery;
mod delivery_lines;
mod delivery_posting;
pub(crate) mod document_number;
mod dto;
mod electronic_delivery;
mod electronic_delivery_crypto;
mod purchase_context;
mod purchase_receipt;
mod purchase_receipt_lines;
mod purchase_receipt_posting;
mod service_fulfillment;
mod service_fulfillment_confirm;
mod service_fulfillment_crypto;
pub(crate) mod task;

pub use self::dto::{
    AcceptanceAllocationInput, AcceptanceAllocationView, AcceptanceEligibilityView, AcceptanceLineInput,
    AcceptanceSalesLineGroupView, CommitCustomerAcceptanceRequest, CommitCustomerAcceptanceView,
    ConfirmServiceFulfillmentRequest, CreateCustomerAcceptanceRequest, CreateDeliveryRequest,
    CreateElectronicDeliveryRequest, CreatePurchaseReceiptRequest, CreateServiceFulfillmentRequest,
    CustomerAcceptanceDetailView, CustomerAcceptanceLineView, CustomerAcceptanceListParams,
    CustomerAcceptanceView, DeliveryDetailView, DeliveryLineInput, DeliveryLineView, DeliveryListParams,
    DeliveryView, ElectronicDeliveryListParams, ElectronicDeliveryView, EligibleFulfillmentFactView,
    PageView, PostAcceptanceLineInput, PostCustomerAcceptanceRequest, PostDeliveryRequest,
    PostPurchaseReceiptRequest, PurchaseReceiptDetailView, PurchaseReceiptLineInput, PurchaseReceiptLineView,
    PurchaseReceiptListParams, PurchaseReceiptView, ReverseCustomerAcceptanceRequest,
    ServiceFulfillmentListParams, ServiceFulfillmentView, UpdateDeliveryRequest,
    UpdatePurchaseReceiptRequest,
};

/// 履约服务。
///
/// 提供采购入库、发货（仓发/直发）、电子交付、服务履约、客户验收的
/// 查询与过账编排；`fingerprint_key` 用于对履约对象快照计算查询指纹
/// （§4.5.5，带密钥 HMAC，密钥不持久化），`sensitive_data` 用于在服务端
/// 加密现场地址等敏感输入。
pub struct FulfillmentService {
    db: Database,
    fingerprint_key: Vec<u8>,
    sensitive_data: Arc<SensitiveDataCodec>,
    rbac: SharedRbacService,
}

impl FulfillmentService {
    /// 创建履约服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `fingerprint_key` - 履约对象快照查询指纹密钥（取 `app.secret` 字节）
    /// * `sensitive_data` - 启动期创建并共享的敏感数据编解码器
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, fingerprint_key: Vec<u8>, sensitive_data: Arc<SensitiveDataCodec>) -> Self {
        let rbac = iam::shared_rbac_service(db.clone());
        Self {
            db,
            fingerprint_key,
            sensitive_data,
            rbac,
        }
    }
}
