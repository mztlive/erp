//! 发单资格声明与付款核销分别建模；占用不代表已付。

use std::fmt;
use std::num::NonZeroU32;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::common::{ConnectorResult, Page, Snapshot};
use crate::Result;

/// 适配器的资金执行要求，由装配事实决定，不能由采购请求降级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderFundingRequirement {
    /// 既有供应商沿用原普通实付门槛。
    PostedPaymentGate,
    /// 本执行组全部批准商品及运费必须有真实到账、有效授权及专属资金保障。
    VerifiedSupplierDeposit,
}

/// 资金核验只接收业务身份，账户归属必须由服务端加载。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderFundsQuery {
    pub execution_group_id: String,
    pub connection_id: String,
    pub supplier_id: String,
    pub purchase_order_id: String,
    pub purchase_revision_id: String,
    pub approved_total: Amount,
}

/// 资金核验回执不表示已付，正式付款仍由核销事实统计。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderFundsCoverage {
    pub reservation_id: String,
    pub account_id: String,
    pub approved_total: Amount,
}

/// erp-supply 消费的窄资金资格接口，不依赖财务领域。
#[async_trait]
pub trait SupplierOrderFunds: Send + Sync {
    /// 核对本执行组全额、专属且当前有效的预存款保障。
    /// # 参数
    /// * `query` - 冻结采购、连接、执行组和预算。
    /// * `executor` - 当前用例执行器；不执行供应商 HTTP。
    /// # 返回
    /// 返回有效资金保障回执。
    /// # 错误
    /// 无占用、授权失效、跨账号、归属或版本变化时拒绝。
    async fn execution_coverage(
        &self,
        query: &OrderFundsQuery,
        executor: &mut dyn Executor,
    ) -> Result<OrderFundsCoverage>;
}

/// 币种必须由本账号合同核验；来源缺失币种时不能使用未经核实的默认值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupplierFundsCurrency {
    CNY,
}

/// 来源业务码与收支方向均已通过受控账号协议核验，未核验记录不能构造正式交易。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupplierFundsDirection {
    Recharge,
    Debit,
    Return,
}

/// 标准Port只携带加密原始证据，不携带任意来源JSON或账户个人资料。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierFundsEvidence {
    pub reference: String,
    pub body_encrypted: String,
    pub content_fingerprint: String,
}

impl fmt::Debug for SupplierFundsEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupplierFundsEvidence")
            .field("reference", &self.reference)
            .field("body_encrypted", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierFundsBalance {
    pub account_reference: String,
    pub amount: Amount,
    pub currency: SupplierFundsCurrency,
    pub evidence: SupplierFundsEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierFundsQuery {
    pub since: Instant,
    pub through: Instant,
    pub cursor: Option<String>,
    pub limit: NonZeroU32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierFundsTransaction {
    pub stable_transaction_id: String,
    pub external_order_id: Option<String>,
    pub direction: SupplierFundsDirection,
    pub amount: Amount,
    pub currency: SupplierFundsCurrency,
    pub occurred_at: Instant,
    pub evidence_reference: String,
    pub original_transaction_id: Option<String>,
    pub bank_reference: Option<String>,
    pub business_code: String,
}

/// 缺少身份、方向、币种或时间依据时只保存未决证据，不猜测为扣款或充值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierFundsUnresolved {
    pub observation_id: String,
    pub evidence_reference: String,
    pub external_transaction_id: Option<String>,
    pub external_order_id: Option<String>,
    pub amount: Option<Amount>,
    pub occurred_at: Option<Instant>,
    pub business_code: Option<String>,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupplierFundsObservationResult {
    Transaction(SupplierFundsTransaction),
    Unresolved(SupplierFundsUnresolved),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierFundsObservation {
    pub evidence: SupplierFundsEvidence,
    pub result: SupplierFundsObservationResult,
}

/// 绑定真实技术资金账号的只读证据来源；所有财务应用仍由拥有领域核验。
#[async_trait]
pub trait SupplierFundsSource: Send + Sync {
    /// 返回已经核实的外部账号引用，不使用前端提交值切换账号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 服务端绑定的稳定资金账号引用。
    /// # 错误
    /// 不返回错误。
    fn account_reference(&self) -> &str;

    /// 声明已核验消息线索需要调查的资金方向，本声明不证明资金已经发生。
    /// # 参数
    /// `message_type` 为相同绑定协议的来源消息类型。
    /// # 返回
    /// 仅已有账号协议明确规定的调查方向；支付失败及未知类型返回None。
    /// # 错误
    /// 不返回错误；None不允许根据空查询自动确认资金调查完成。
    fn message_effect(&self, _message_type: &str) -> Option<SupplierFundsDirection> {
        None
    }

    /// 采样当前真实账户余额，仅用于余额预警与对账。
    /// # 参数
    /// 无。
    /// # 返回
    /// 原币种余额和加密证据；读取时刻不冒充账务发生时刻。
    /// # 错误
    /// 无权限、缺币种或响应缺失时失败，不返回零余额。
    async fn balance(&self) -> ConnectorResult<Snapshot<SupplierFundsBalance>>;

    /// 按冻结时间窗口与绑定游标读取资金变动，不在适配器中登记付款。
    /// # 参数
    /// `query` 固定来源、时间窗口与分页上限，恢复时不得改变。
    /// # 返回
    /// 带加密来源证据的已核实或未决交易；页结束不保证远端一致快照。
    /// # 错误
    /// 账号、游标、查询范围或传输失败时拒绝，失败页不得推进检查点。
    async fn transactions(
        &self,
        query: &SupplierFundsQuery,
    ) -> ConnectorResult<Page<Snapshot<SupplierFundsObservation>>>;
}
