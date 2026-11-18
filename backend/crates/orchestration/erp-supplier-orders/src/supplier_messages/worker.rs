use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use erp_party::SensitiveDataCodec;
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::repository::supplier_catalog;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;
use tokio::sync::Mutex;

use super::refresh::SupplierMessageRefresh;
use super::{
    MongoSupplierMessageStore, SupplierMessageCatalog, SupplierMessageFundsInvestigation,
    SupplierMessageProcess,
};
use crate::{Error, Result};

/// 按已登记协议及账号租期消费消息，新增下单开关不影响既有消息调查。
pub struct SupplierMessageWorker {
    db: Database,
    registry: Arc<SupplierConnectorRegistry>,
    codec: SensitiveDataCodec,
    cursor: Mutex<String>,
    polls: Mutex<PollSchedule>,
    funds: Option<Arc<dyn SupplierMessageFundsInvestigation>>,
    catalog: Arc<dyn SupplierMessageCatalog>,
}

impl SupplierMessageWorker {
    /// 复用应用数据库、实际协议注册表与敏感证据加密器。
    /// # 参数
    /// `db`、`registry`、`codec` 均由应用组合根注入。
    /// # 返回
    /// 未发起外部请求的消息worker。
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        registry: Arc<SupplierConnectorRegistry>,
        codec: SensitiveDataCodec,
        catalog: Arc<dyn SupplierMessageCatalog>,
    ) -> Self {
        Self {
            db,
            registry,
            codec,
            cursor: Mutex::new(String::new()),
            polls: Mutex::new(PollSchedule::default()),
            funds: None,
            catalog,
        }
    }

    /// 注入财务资金调查实现，缺失时退款及支付异常消息保持待核实。
    /// # 参数
    /// `funds` 由组合根绑定真实账号及资金证据应用流程。
    /// # 返回
    /// 带资金调查能力的消息worker。
    /// # 错误
    /// 不返回错误，不执行查询或记账。
    pub fn with_funds_investigation(mut self, funds: Arc<dyn SupplierMessageFundsInvestigation>) -> Self {
        self.funds = Some(funds);
        self
    }

    /// 有界读取20个连接，每个具有实际消息能力的账号最多接收20条消息。
    /// # 参数
    /// 无；账号和消息能力只从服务端注册适配器取得。
    /// # 返回
    /// 本轮完成远端确认的消息数；隔离与未知结果保留记录并告警。
    /// # 错误
    /// 连接分页失败时返回错误；单连接错误保留消息且不阻断其余连接。
    pub async fn run_once(&self) -> Result<usize> {
        let mut cursor = self.cursor.lock().await;
        let kinds = self.registry.connector_types();
        let connections =
            supplier_catalog::connection_page(&self.db, &cursor, &kinds, &mut NoTransaction).await?;
        if connections.is_empty() {
            cursor.clear();
            return Ok(0);
        }

        let mut acknowledged = 0;
        for connection in connections {
            *cursor = connection.base.id.clone();
            match self.run_connection(&connection).await {
                Ok(count) => acknowledged += count,
                Err(_) => tracing::warn!(connection_id = %connection.base.id,
                    "供应商消息处理受阻，证据及未确认消息保留，需核对连接或补查异常"),
            }
        }
        Ok(acknowledged)
    }

    async fn run_connection(&self, connection: &SupplierApiConnection) -> Result<usize> {
        let connector = self
            .registry
            .notification_connector(connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let Some(source) = connector.messages() else {
            return Ok(0);
        };
        if connector.connection_id().as_ref() != connection.base.id {
            return Err(Error::Forbidden("供应商消息能力与当前连接归属不一致".into()));
        }

        if !self.polls.lock().await.claim(source.account_scope(), source.poll_interval(), Instant::now()) {
            return Ok(0);
        }
        let Some(store) = MongoSupplierMessageStore::claim(
            self.db.clone(),
            source.account_scope(),
            connector.connection_id().clone(),
            &next_id(),
            300,
        )
        .await?
        else {
            return Ok(0);
        };

        let applier = SupplierMessageRefresh::new(
            self.db.clone(),
            self.registry.clone(),
            connection.clone(),
            self.funds.clone(),
            self.catalog.clone(),
        );
        let outcome = SupplierMessageProcess { store: &store, source, applier: &applier, codec: &self.codec }
            .run(NonZeroU32::new(20).expect("固定非零批次"))
            .await;
        store.release().await?;
        let result = outcome?;

        if result.quarantined + result.deferred + result.acknowledgement_unknown > 0 {
            tracing::warn!(connection_id = %connection.base.id, quarantined = result.quarantined,
                deferred = result.deferred, ack_unknown = result.acknowledgement_unknown,
                "供应商消息有未结束处理项，请核对来源证据及补查进度");
        }
        Ok(result.acknowledged)
    }
}

#[derive(Default)]
struct PollSchedule(BTreeMap<String, Instant>);
impl PollSchedule {
    fn claim(&mut self, account: &str, interval: Duration, now: Instant) -> bool {
        if self.0.get(account).is_some_and(|next| *next > now) {
            return false;
        }
        let interval = interval.clamp(Duration::from_secs(1), Duration::from_secs(86400));
        self.0.insert(account.into(), now + interval);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_interval_is_respected_without_blocking_other_accounts() {
        let mut schedule = PollSchedule::default();
        let now = Instant::now();
        let period = Duration::from_secs(60);
        assert!(schedule.claim("a", period, now));
        assert!(!schedule.claim("a", period, now + Duration::from_secs(5)));
        assert!(schedule.claim("b", period, now + Duration::from_secs(5)));
        assert!(schedule.claim("a", period, now + period));
    }
}
