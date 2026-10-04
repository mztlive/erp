//! 履约 HTTP 入口与共享命令装配，按交付方式划分协议适配。

pub mod acceptance_evidence;
pub mod customer_acceptance;
pub mod delivery;
pub mod electronic_delivery;
mod evidence_upload;
pub mod purchase_receipt;
pub mod service_fulfillment;

use erp_fulfillment::service::FulfillmentService;
use erp_processes::fulfillment_execution::FulfillmentProcess;

use crate::app_state::AppState;

/// 构造本域履约查询服务。
///
/// # 参数
/// * `state` - 应用状态
///
/// # 返回
/// 返回履约服务实例。
fn service(state: &AppState) -> FulfillmentService {
    FulfillmentService::new(state.db())
}

/// 构造跨域履约过程并保留实际配置和对象读取授权注入。
fn process(state: &AppState) -> FulfillmentProcess {
    FulfillmentProcess::new(
        state.db(),
        state.config_snapshot().app.secret.as_bytes().to_vec(),
        state.sensitive_data(),
    )
    .with_object_read(state.approval_object_read())
}
