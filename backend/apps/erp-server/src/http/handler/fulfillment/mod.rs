//! 履约 HTTP 入口与共享命令装配，按交付方式划分协议适配。

pub mod acceptance_evidence;
pub mod customer_acceptance;
pub mod delivery;
pub mod electronic_delivery;
mod evidence_upload;
pub mod purchase_receipt;
pub mod service_fulfillment;

use std::sync::Arc;

use erp_fulfillment::service::FulfillmentReadService;
use erp_fulfillment_execution::fulfillment_execution::FulfillmentProcess;
use erp_fulfillment_execution::workflow::FulfillmentWorkflowServices;

use crate::http::state::HttpState;
use crate::wiring::approval_runtime;
use crate::wiring::workflow::work_item_service;

// 构造本域履约查询服务。
// 返回履约服务实例。
fn service(state: &HttpState) -> FulfillmentReadService {
    FulfillmentReadService::new(state.application().db(), state.application().authorization())
}

// 为跨域履约命令构造 Process，接入应用实际配置与对象读取授权，而非在 HTTP 侧执行业务规则。
fn process(state: &HttpState) -> FulfillmentProcess {
    FulfillmentProcess::new(
        state.application().db(),
        state.application().config_snapshot().app.secret.as_bytes().to_vec(),
        state.application().sensitive_data(),
        state.application().authorization(),
        Arc::new(FulfillmentWorkflowServices::new(
            work_item_service(
                state.application().db(),
                state.application().rbac(),
                state.application().authorization(),
            ),
            approval_runtime(
                state.application().db(),
                state.application().rbac(),
                state.application().authorization(),
            ),
        )),
    )
}

pub mod address_arrangement;
