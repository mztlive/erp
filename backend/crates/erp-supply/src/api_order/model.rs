use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 每个远端写步骤在发送前持久化进行中状态；重启后只能调查原动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiOrderStage {
    Queued,
    AwaitingSetup,
    PrepareAddress,
    AddressUnknown,
    PlanDelivery,
    Create,
    CreationUnknown,
    ConfirmPayment,
    PaymentUnknown,
    Observe,
    Stopped,
}

/// 已批准采购行与供应商执行明细的永久关联。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiOrderSourceLine {
    pub fulfillment_item_id: String,
    pub purchase_order_revision_line_id: String,
    pub sales_order_line_id: Option<String>,
    pub supplier_offering_revision_id: String,
    pub quantity: Quantity,
    pub delivery_allocation_index: u32,
}

/// 与既有供应商订单同身份；协议请求、地址及配送令牌禁止明文落库。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct ApiOrderExecution {
    #[serde(flatten)]
    pub base: BaseModel,
    pub purchase_order_id: String,
    pub purchase_order_revision_id: String,
    pub connection_id: String,
    pub supplier_id: String,
    pub source_lines: Vec<ApiOrderSourceLine>,
    pub stage: ApiOrderStage,
    pub request_encrypted: Option<String>,
    pub address_fingerprint: Option<String>,
    pub blocker: Option<String>,
    pub next_run_at: Instant,
    pub setup_idempotency_key: Option<String>,
    pub setup_payload_hash: Option<String>,
    #[serde(default)]
    pub warehouse_revision_id: Option<String>,
    #[serde(default)]
    pub action_hashes: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub superseded_by_purchase_revision_id: Option<String>,
}

impl ApiOrderExecution {
    /// 为正式采购建立尚未发送任何外部请求的执行意图。
    /// # 参数
    /// 身份由正式采购版本、连接与地址分组决定；明细来自冻结采购行。
    /// # 返回
    /// 待执行记录，创建后由后台逐步处理。
    /// # 错误
    /// 不返回错误；调用方须先核验正式采购事实。
    pub fn queued(
        id: String,
        purchase_order_id: String,
        purchase_order_revision_id: String,
        connection_id: String,
        supplier_id: String,
        source_lines: Vec<ApiOrderSourceLine>,
    ) -> Self {
        Self {
            base: BaseModel::new(id),
            purchase_order_id,
            purchase_order_revision_id,
            connection_id,
            supplier_id,
            source_lines,
            stage: ApiOrderStage::Queued,
            request_encrypted: None,
            address_fingerprint: None,
            blocker: None,
            next_run_at: Instant::now(),
            setup_idempotency_key: None,
            setup_payload_hash: None,
            action_hashes: std::collections::BTreeMap::new(),
            superseded_by_purchase_revision_id: None,
            warehouse_revision_id: None,
        }
    }

    /// 标记等待人工补充或核查；不自动重放可能已送达的远端写入。
    /// # 参数
    /// `reason` 为不含地址、电话或协议密钥的业务提示。
    /// # 返回
    /// 当前步骤保留，后续定时读取使用有界退避。
    /// # 错误
    /// 不返回错误。
    pub fn defer(&mut self, reason: impl Into<String>) {
        self.blocker = Some(reason.into());
        self.next_run_at = Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(60));
    }

    /// 核对当前执行尚未进入可能创建供应商订单的步骤。
    /// # 参数
    /// 无；仅使用已持久化的步骤及动作指纹。
    /// # 返回
    /// 未发送创建动作时允许采购变更停止此执行。
    /// # 错误
    /// 创建动作已经登记、结果未知或进入后续履约步骤时拒绝。
    pub fn ensure_unsent_for_purchase_change(&self) -> Result<()> {
        let unsent_stage = matches!(
            self.stage,
            ApiOrderStage::Queued
                | ApiOrderStage::AwaitingSetup
                | ApiOrderStage::PrepareAddress
                | ApiOrderStage::AddressUnknown
                | ApiOrderStage::PlanDelivery
                | ApiOrderStage::Create
                | ApiOrderStage::Stopped
        );
        if !unsent_stage || self.action_hashes.contains_key("create") {
            return Err(Error::BusinessLogicError(
                "供应商订单已发送或创建结果尚未查明；当前协议不支持自动改单，请先处理原供应商订单".into(),
            ));
        }
        Ok(())
    }

    /// 在采购变更事务内停止未发送的旧执行，保留原请求与来源。
    /// # 参数
    /// `replacement_revision` 为将要生效的采购修订；调用方仍须核对没有外部单号。
    /// # 返回
    /// 原意图进入停止状态，不再被后台执行器领取。
    /// # 错误
    /// 已登记创建动作或状态不能证明未发单时拒绝。
    pub fn stop_for_purchase_change(&mut self, replacement_revision: &str) -> Result<()> {
        self.ensure_unsent_for_purchase_change()?;
        self.mark_superseded(replacement_revision)
    }

    /// 记录已经通过未发或可信取消核验的执行替代关系。
    /// # 参数
    /// `replacement_revision` 为已核验的新采购修订身份。
    /// # 返回
    /// 同修订重放不重复改变，首次记录后停止后台执行。
    /// # 错误
    /// 无效修订或已由其他修订替代时拒绝。
    pub(super) fn mark_superseded(&mut self, replacement_revision: &str) -> Result<()> {
        if replacement_revision.trim().is_empty() || replacement_revision == self.purchase_order_revision_id {
            return Err(Error::ValidationError("替代采购版本无效".into()));
        }
        if let Some(previous) = &self.superseded_by_purchase_revision_id {
            if previous != replacement_revision {
                return Err(Error::ConflictError("原接口执行已经由其他采购版本替代".into()));
            }
            return Ok(());
        }
        self.superseded_by_purchase_revision_id = Some(replacement_revision.into());
        self.stage = ApiOrderStage::Stopped;
        self.blocker = Some("采购变更已替代此已安全停止的执行，原请求及交付记录保留备查".into());
        Ok(())
    }
}

#[cfg(test)]
mod purchase_change_tests {
    use super::*;

    fn execution(stage: ApiOrderStage) -> ApiOrderExecution {
        let mut execution =
            ApiOrderExecution::queued("e".into(), "p".into(), "r".into(), "c".into(), "s".into(), vec![]);
        execution.stage = stage;
        execution
    }

    #[test]
    fn every_unsent_step_can_stop_without_erasing_original_request() {
        for stage in [
            ApiOrderStage::Queued,
            ApiOrderStage::AwaitingSetup,
            ApiOrderStage::PrepareAddress,
            ApiOrderStage::AddressUnknown,
            ApiOrderStage::PlanDelivery,
            ApiOrderStage::Create,
            ApiOrderStage::Stopped,
        ] {
            let mut value = execution(stage);
            value.request_encrypted = Some("original encrypted request".into());
            value.action_hashes.insert("address".into(), "address fingerprint".into());
            value.stop_for_purchase_change("new-revision").unwrap();
            assert_eq!(value.stage, ApiOrderStage::Stopped);
            assert_eq!(value.request_encrypted.as_deref(), Some("original encrypted request"));
            assert_eq!(value.action_hashes["address"], "address fingerprint");
            assert!(value.blocker.is_some());
            assert_eq!(value.superseded_by_purchase_revision_id.as_deref(), Some("new-revision"));
        }
    }

    #[test]
    fn possible_order_creation_and_later_steps_cannot_be_superseded() {
        for stage in [
            ApiOrderStage::CreationUnknown,
            ApiOrderStage::ConfirmPayment,
            ApiOrderStage::PaymentUnknown,
            ApiOrderStage::Observe,
        ] {
            let mut value = execution(stage);
            assert!(value.stop_for_purchase_change("new-revision").is_err());
            assert_eq!(value.stage, stage);
        }
        for stage in [ApiOrderStage::Create, ApiOrderStage::Stopped, ApiOrderStage::AwaitingSetup] {
            let mut value = execution(stage);
            value.action_hashes.insert("create".into(), "original create fingerprint".into());
            assert!(value.stop_for_purchase_change("new-revision").is_err());
            assert_eq!(value.stage, stage);
        }
    }
}
