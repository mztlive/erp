//! 拉取消息可靠接收、定向补查及远端确认；不复用已验签回调语义。

mod address;
mod intake;
mod persistence;
mod refresh;
#[cfg(test)]
mod tests;
mod worker;

use std::num::NonZeroU32;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_integration::entity::supplier_callback::SupplierRefreshHint;
use erp_integration::entity::supplier_message::{SupplierMessageState, SupplierPulledMessage};
use erp_party::SensitiveDataCodec;
use erp_supply::ports::connector::messages::{MessageAcknowledgement, SupplierMessageSource};
pub use persistence::MongoSupplierMessageStore;
pub use worker::SupplierMessageWorker;

use crate::{Error, Result};

/// 当前账号消费资格下的可靠存储；实现须原子校验账号围栏及消息版本。
#[async_trait]
pub trait SupplierMessageStore: Send + Sync {
    /// 返回唯一绑定的消费账号与连接。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已持久化确认的服务端归属。
    /// # 错误
    /// 不返回错误。
    fn binding(&self) -> (&str, &SupplierApiConnectionId);

    /// 原子保存原始证据和补查意图，同号异载荷须保存隔离结果。
    /// # 参数
    /// `message` 为密文接收记录。
    /// # 返回
    /// 原接收状态或新记录；重复不得重置完成状态。
    /// # 错误
    /// 保存、归属或账号围栏失败时不得继续远端确认。
    async fn receive(&self, message: SupplierPulledMessage) -> Result<SupplierPulledMessage>;

    /// 读取当前账号与连接的有界恢复队列。
    /// # 参数
    /// 无。
    /// # 返回
    /// 待补查、待确认及发送中记录。
    /// # 错误
    /// 失去账号消费资格或读取失败时拒绝。
    async fn pending(&self) -> Result<Vec<SupplierPulledMessage>>;

    /// 原子校验账号资格及消息版本后保存处理进度。
    /// # 参数
    /// `message` 为当前版本，写入成功同步新版本。
    /// # 返回
    /// 处理进度已经可靠提交。
    /// # 错误
    /// 版本冲突、租期失效或数据库失败时停止执行。
    async fn save(&self, message: &mut SupplierPulledMessage) -> Result<()>;

    /// 外部调用之前重验账号消费资格。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前处理仍可执行。
    /// # 错误
    /// 过期或被替代时拒绝。
    async fn check_lease(&self) -> Result<()>;
}

/// 按消息稳定身份补查最新事实；应用必须使用领域幂等机制，不能直接信任消息正文。
#[async_trait]
pub trait SupplierMessageRefreshApplier: Send + Sync {
    /// 执行全部定向查询并按原领域合同应用，失败保留可恢复意图。
    /// # 参数
    /// `message` 已可靠保存，账号与连接已经校验；只消费 refresh_hints。
    /// # 返回
    /// 全部意图已应用或已有持久化的明确隔离结论。
    /// # 错误
    /// 查询、权限、版本或正式应用失败时拒绝结束；重放同消息不得重复履约或记账。
    async fn refresh(&self, message: &SupplierPulledMessage) -> Result<()>;
}

/// 财务拥有的资金调查入口；只允许在真实证据已经持久化后结束消息线索。
#[async_trait]
pub trait SupplierMessageFundsInvestigation: Send + Sync {
    /// 补查原账号、订单及资金明细，并确认该线索已有可追溯处理结论。
    /// # 参数
    /// `message`、`hint` 来自已可靠接收且已核验归属的消息，不是扣款或到账证明。
    /// # 返回
    /// 真实资金查询及按领域规则的持久化处理完成后返回成功。
    /// # 错误
    /// 未定位原单、证据缺失、状态未知或财务应用未结束时必须返回错误，不能根据消息直接记账。
    async fn investigate(&self, message: &SupplierPulledMessage, hint: &SupplierRefreshHint) -> Result<()>;
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SupplierMessageRun {
    pub received: usize,
    pub applied: usize,
    pub acknowledged: usize,
    pub quarantined: usize,
    pub deferred: usize,
    pub acknowledgement_unknown: usize,
}

/// 账号级消费编排；HTTP 永远在本地事务之外。
pub struct SupplierMessageProcess<'a> {
    pub store: &'a dyn SupplierMessageStore,
    pub source: &'a dyn SupplierMessageSource,
    pub applier: &'a dyn SupplierMessageRefreshApplier,
    pub codec: &'a SensitiveDataCodec,
}

impl SupplierMessageProcess<'_> {
    /// 拉取并可靠保存后恢复处理本地队列，最后单次确认远端消息。
    /// # 参数
    /// `limit` 为本轮远端拉取上限，最大50。
    /// # 返回
    /// 各阶段计数；隔离、延迟及未知确认分别计数。
    /// # 错误
    /// 归属、存储或账号围栏失败时立即停止；远端读取失败不冒充空队列。
    pub async fn run(&self, limit: NonZeroU32) -> Result<SupplierMessageRun> {
        if limit.get() > 50 || self.source.account_scope() != self.store.binding().0 {
            return Err(Error::ValidationError("供应商消息账号归属或批次数量无效".into()));
        }
        self.store.check_lease().await?;
        let mut result = self.resume_pending().await?;
        let pulled = self.source.pull(limit).await.map_err(|error| {
            Error::BusinessLogicError(format!("供应商消息读取失败：{} ({})", error.summary, error.code))
        })?;
        if pulled.len() > usize::try_from(limit.get()).expect("u32批次不超过50") {
            return Err(Error::BusinessLogicError("供应商消息响应超过批次上限".into()));
        }
        for incoming in pulled {
            let (account, connection) = self.store.binding();
            let message = intake::receive(account, connection, incoming, self.codec, Instant::now())?;
            let saved = self.store.receive(message).await?;
            result.received += 1;
            result.quarantined += usize::from(saved.state == SupplierMessageState::Quarantined);
        }
        for mut message in self.store.pending().await? {
            self.process_one(&mut message, &mut result).await?;
        }
        Ok(result)
    }

    /// 仅恢复可靠接收的本地消息，不依赖远端拉取请求成功。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本轮补查和确认结果；发送中记录恢复为未知，禁止重发。
    /// # 错误
    /// 账号绑定、租期或持久化失败时停止。
    pub async fn resume_pending(&self) -> Result<SupplierMessageRun> {
        if self.source.account_scope() != self.store.binding().0 {
            return Err(Error::Forbidden("供应商消息账号归属不一致".into()));
        }
        self.store.check_lease().await?;
        let mut result = SupplierMessageRun::default();
        for mut message in self.store.pending().await? {
            self.process_one(&mut message, &mut result).await?;
        }
        Ok(result)
    }

    async fn process_one(
        &self,
        message: &mut SupplierPulledMessage,
        result: &mut SupplierMessageRun,
    ) -> Result<()> {
        self.verify_binding(message)?;
        match message.state {
            SupplierMessageState::Received | SupplierMessageState::Applying => {
                if !self.apply(message, result).await? {
                    return Ok(());
                }
            },
            SupplierMessageState::AckSending => {
                message.acknowledged(false, Instant::now())?;
                self.store.save(message).await?;
                result.acknowledgement_unknown += 1;
                return Ok(());
            },
            SupplierMessageState::AckPending => {},
            _ => return Ok(()),
        }
        self.acknowledge(message, result).await
    }

    async fn apply(
        &self,
        message: &mut SupplierPulledMessage,
        result: &mut SupplierMessageRun,
    ) -> Result<bool> {
        message.begin_apply()?;
        self.store.save(message).await?;
        self.store.check_lease().await?;
        if self.applier.refresh(message).await.is_err() {
            let retry = Instant::try_from_unix_secs(Instant::now().unix_secs().saturating_add(120))?;
            message.defer_application(retry, "REFRESH_APPLICATION_FAILED".into())?;
            self.store.save(message).await?;
            result.deferred += 1;
            return Ok(false);
        }
        message.applied(Instant::now())?;
        self.store.save(message).await?;
        result.applied += 1;
        Ok(true)
    }

    async fn acknowledge(
        &self,
        message: &mut SupplierPulledMessage,
        result: &mut SupplierMessageRun,
    ) -> Result<()> {
        message.begin_acknowledge()?;
        self.store.save(message).await?;
        self.store.check_lease().await?;
        let confirmed = matches!(
            self.source.acknowledge(&message.source_message_id).await,
            Ok(MessageAcknowledgement::Confirmed)
        );
        message.acknowledged(confirmed, Instant::now())?;
        self.store.save(message).await?;
        if confirmed {
            result.acknowledged += 1;
        } else {
            result.acknowledgement_unknown += 1;
        }
        Ok(())
    }

    fn verify_binding(&self, message: &SupplierPulledMessage) -> Result<()> {
        let (account, connection) = self.store.binding();
        if message.account_scope != account || &message.connection_id != connection {
            return Err(Error::Forbidden("供应商消息不属于当前消费连接".into()));
        }
        Ok(())
    }
}
