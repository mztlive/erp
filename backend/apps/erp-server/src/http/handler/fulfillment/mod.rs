//! 履约 HTTP 入口与共享命令装配，按交付方式划分协议适配。

pub mod acceptance_evidence;
pub mod customer_acceptance;
pub mod delivery;
pub mod electronic_delivery;
mod evidence_upload;
pub mod purchase_receipt;
pub mod service_fulfillment;

use std::sync::Arc;

use erp_fulfillment::service::FulfillmentService;
use erp_fulfillment_execution::fulfillment_execution::FulfillmentProcess;
use erp_fulfillment_execution::workflow::FulfillmentWorkflowServices;

use crate::http::state::HttpState;
use crate::wiring::identity::shared_rbac_service;
use crate::wiring::workflow::work_item_service;
use crate::wiring::{approval_runtime, fulfillment_identity_factory};

/// 构造本域履约查询服务。
///
/// # 参数
/// * `state` - 应用状态
///
/// # 返回
/// 返回履约服务实例。
fn service(state: &HttpState) -> FulfillmentService {
    FulfillmentService::new(state.application().db())
}

/// 构造跨域履约过程并保留实际配置和对象读取授权注入。
fn process(state: &HttpState) -> FulfillmentProcess {
    let rbac = shared_rbac_service(state.application().db());
    FulfillmentProcess::new(
        state.application().db(),
        state.application().config_snapshot().app.secret.as_bytes().to_vec(),
        state.application().sensitive_data(),
        fulfillment_identity_factory(state.application().db()),
        Arc::new(FulfillmentWorkflowServices::new(
            work_item_service(state.application().db(), rbac.clone()),
            approval_runtime(state.application().db(), rbac),
        )),
    )
}

pub mod address_arrangement;
