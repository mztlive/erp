//! 只读请求白名单；认证实现必须独立取得账号协议证据后注入。
use std::future::Future;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{Instant, sleep_until, timeout};

use super::error;
use super::order_payload::SubmitOrder;
use super::wire::{WireText, decode};

/// 仅列入已核验官方文档的读取方法，不能携带创建、确认或删除命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JdVopReadRequest {
    Pools,
    PoolPage {
        pool_id: String,
        offset: String,
        page_size: NonZeroU32,
    },
    Detail {
        sku_id: String,
        /// 官方详情扩展 13 请求商品购买倍数，不从包装参数推断。
        query_ext_set: Vec<u32>,
    },
    Price {
        sku_id: String,
        quantity: String,
    },
    Images {
        sku_id: String,
    },
    Categories {
        category_ids: Vec<String>,
    },
    Areas {
        level: u8,
        parent_code: Option<String>,
    },
    VerifyAddress {
        province: String,
        city: String,
        county: String,
        town: String,
    },
    Order {
        merchant_order_no: Option<String>,
        external_order_no: Option<String>,
    },
    Logistics {
        external_order_no: String,
    },
    DeliveryInfo {
        external_order_no: String,
        source_type: u32,
    },
    CancellationEligibility {
        external_order_nos: Vec<String>,
    },
    CancelProgress {
        external_order_no: String,
    },
    SaleEligibility {
        lines: Vec<(String, u32)>,
        area: [String; 4],
    },
    Stock {
        lines: Vec<(String, u32)>,
        area: [String; 4],
    },
    Freight {
        lines: Vec<(String, u32)>,
        area: [String; 4],
        payment_type: u8,
    },
    Balance,
    FundsBalance {
        source_type: u32,
    },
    BalanceChanges {
        source_type: u32,
        start_date: String,
        end_date: String,
        page_size: NonZeroU32,
        page_index: u32,
    },
    Messages {
        types: Vec<u32>,
        read_type: u8,
        source_type: u32,
    },
}

impl JdVopReadRequest {
    /// 返回请求对应的官方方法名。
    /// # 参数
    /// 无。
    /// # 返回
    /// 固定白名单内的方法名。
    /// # 错误
    /// 不返回错误。
    pub fn method(&self) -> &'static str {
        match self {
            Self::Pools => "jingdong.vop.goods.getSkuPoolInfo",
            Self::PoolPage { .. } => "jingdong.vop.goods.querySkuByPage",
            Self::Detail { .. } => "jingdong.vop.goods.getSkuDetailInfo",
            Self::Price { .. } => "jingdong.vop.goods.getSellPrice",
            Self::Images { .. } => "jingdong.vop.goods.getSkuImageList",
            Self::Categories { .. } => "jingdong.vop.goods.getCategoryInfoList",
            Self::Areas { .. } => "jingdong.vop.address.queryJdAreaIdList",
            Self::VerifyAddress { .. } => "jingdong.vop.address.verifyAreaFourIdOpenReq",
            Self::Order { .. } => "jingdong.vop.order.queryOrderDetail",
            Self::Logistics { .. } => "jingdong.vop.order.getTrackShowResultForShow.query",
            Self::DeliveryInfo { .. } => "jingdong.vop.order.queryDeliveryInfo",
            Self::CancellationEligibility { .. } => "jingdong.vop.order.batchCheckCancelOrder",
            Self::CancelProgress { .. } => {
                "jingdong.vop.order.queryOrderOpenProvider.getOrderCancelProcessDetail"
            },
            Self::SaleEligibility { .. } => "jingdong.vop.goods.getSkusAllSaleState",
            Self::Stock { .. } => "jingdong.vop.goods.getNewStockById",
            Self::Freight { .. } => "jingdong.vop.order.querySkuFreight",
            Self::Balance => "jingdong.vop.order.checkAccountBalance",
            Self::FundsBalance { .. } => "jingdong.vop.order.checkAccountBalance",
            Self::BalanceChanges { .. } => "jingdong.vop.order.checkBalanceChangeInfo",
            Self::Messages { .. } => "jingdong.vop.message.queryTransByVopNormal",
        }
    }
}

/// 每次调用只对应一个已经持久化的外部写动作，不包含自动重放许可。
#[derive(Clone)]
pub enum JdVopWriteRequest {
    Submit { order: Box<SubmitOrder>, source_type: u32 },
    Confirm { merchant_order_no: String, external_order_no: String, source_type: u32 },
    ConfirmReceipt { merchant_order_no: String, external_order_no: String, source_type: u32 },
    Cancel { merchant_order_no: String, external_order_no: String, reason: String, source_type: u32 },
    DeleteMessages { ids: Vec<String>, source_type: u32 },
}

impl JdVopWriteRequest {
    /// 返回该写方法在官方示例中的 RPC 响应封装键。
    /// # 参数
    /// 无。
    /// # 返回
    /// 创建和消息删除使用 vopOrderRpcResult，确认、取消及确认收货使用 openRpcResult。
    /// # 错误
    /// 不返回错误；新增方法必须显式选择其响应封装。
    pub fn response_key(&self) -> &'static str {
        match self {
            Self::Submit { .. } | Self::DeleteMessages { .. } => "vopOrderRpcResult",
            Self::Confirm { .. } | Self::ConfirmReceipt { .. } | Self::Cancel { .. } => "openRpcResult",
        }
    }
    /// 返回写动作的固定官方方法名。
    /// # 参数
    /// 无。
    /// # 返回
    /// 创建、确认或取消方法名。
    /// # 错误
    /// 不返回错误。
    pub fn method(&self) -> &'static str {
        match self {
            Self::Submit { .. } => "jingdong.vop.order.submitOrder",
            Self::Confirm { .. } => "jingdong.vop.order.confirmOrder",
            Self::ConfirmReceipt { .. } => "jingdong.vop.order.confirmReceiveByOrder",
            Self::Cancel { .. } => "jingdong.vop.order.cancelOrder",
            Self::DeleteMessages { .. } => "jingdong.vop.message.deleteClientMsgByIdList",
        }
    }
}

/// 通过 G0 验收后注入的单次写传输；任何可能已送达的失败必须返回 ResultUnknown。
#[async_trait]
pub trait JdVopWriteTransport: Send + Sync {
    /// 发送一次已冻结动作，不自动重试、拆单、确认其他订单或变更单价。
    /// # 参数
    /// `request` 为当前唯一动作的类型化载荷。
    /// # 返回
    /// 方法完整响应；不把HTTP成功视为业务成功。
    /// # 错误
    /// 传输分类只用于诊断；未取得账号级未写入证明时，客户端统一保留结果未知。
    async fn write(&self, request: &JdVopWriteRequest) -> ConnectorResult<Vec<u8>>;
}

pub(super) struct WriteClient {
    pub(super) transport: Arc<dyn JdVopWriteTransport>,
    pub(super) reader: Arc<ReadClient>,
}

impl WriteClient {
    /// 只接受明确的 true 应答，不把业务失败当作未发送证明。
    /// # 参数
    /// `request` 为当前已持久化的原写动作。
    /// # 返回
    /// 单次发送且应答为 true 时成功。
    /// # 错误
    /// 传输、解码或非 true 应答保持原写结果未知分类。
    pub(super) async fn send_true(&self, request: JdVopWriteRequest) -> ConnectorResult<()> {
        let result: WireText = self.send(request).await?;
        if result.0 != "true" {
            return Err(write_unknown());
        }
        Ok(())
    }

    pub(super) async fn send<T: DeserializeOwned>(&self, request: JdVopWriteRequest) -> ConnectorResult<T> {
        let body = self
            .reader
            .with_slot(
                async { self.transport.write(&request).await.map_err(unknown_after_write) },
                write_unknown,
                write_unknown,
            )
            .await?;

        // 业务码或 success=false 不证明副作用未发生，未核验的失败不能释放原写意图。
        decode(&body, request.method(), request.response_key()).map_err(unknown_after_write)
    }
}

pub(super) fn write_unknown() -> ConnectorError {
    error(
        SupplierFailureClass::ResultUnknown,
        "JD_VOP_WRITE_RESULT_UNKNOWN",
        "京东原请求结果待核实，请查询原结果",
    )
}

fn unknown_after_write(failure: ConnectorError) -> ConnectorError {
    let mut unknown = write_unknown();
    if failure.code.starts_with("JD_VOP_")
        && failure.code.len() <= 64
        && failure.code.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        unknown.code = failure.code;
    }
    unknown
}

/// 已完成账号认证协议验收的传输实现；不得将密钥或原始错误正文写入摘要。
#[async_trait]
pub trait JdVopReadTransport: Send + Sync {
    /// 执行一次白名单读取，负责已验签名和 HTTP/网关失败分类，不隐藏写入或重试。
    /// # 参数
    /// `request` 为类型化读取参数；认证、sourceType 及令牌由已验收实现负责。
    /// # 返回
    /// 官方方法完整响应字节，由适配器按方法解码。
    /// # 错误
    /// HTTP、鉴权、配额和读取失败返回脱敏分类错误。
    async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>>;
}

pub(super) struct ReadClient {
    transport: Arc<dyn JdVopReadTransport>,
    semaphore: Semaphore,
    next_request: Mutex<Instant>,
    interval: Duration,
    timeout: Duration,
}

impl ReadClient {
    pub(super) fn new(
        transport: Arc<dyn JdVopReadTransport>,
        concurrency: u32,
        rate: u32,
        seconds: u64,
    ) -> Self {
        Self {
            transport,
            semaphore: Semaphore::new(usize::try_from(concurrency).expect("已校验并发上限20")),
            next_request: Mutex::new(Instant::now()),
            interval: Duration::from_nanos(1_000_000_000 / u64::from(rate)),
            timeout: Duration::from_secs(seconds),
        }
    }

    pub(super) async fn read<T: DeserializeOwned>(&self, request: JdVopReadRequest) -> ConnectorResult<T> {
        let body = self
            .with_slot(
                self.transport.read(&request),
                || {
                    error(
                        SupplierFailureClass::TransientFailure,
                        "JD_VOP_READ_CLOSED",
                        "京东读取服务暂不可用",
                    )
                },
                || {
                    error(
                        SupplierFailureClass::TransientFailure,
                        "JD_VOP_READ_TIMEOUT",
                        "京东读取超时，请稍后重试",
                    )
                },
            )
            .await?;
        decode(&body, request.method(), "openRpcResult")
    }

    async fn with_slot(
        &self,
        request: impl Future<Output = ConnectorResult<Vec<u8>>>,
        slot_unavailable: fn() -> ConnectorError,
        deadline_elapsed: fn() -> ConnectorError,
    ) -> ConnectorResult<Vec<u8>> {
        timeout(self.timeout, async {
            let _permit = self.semaphore.acquire().await.map_err(|_| slot_unavailable())?;
            let scheduled = {
                let mut next = self.next_request.lock().await;
                let scheduled = (*next).max(Instant::now());
                *next = scheduled + self.interval;
                scheduled
            };
            sleep_until(scheduled).await;
            request.await
        })
        .await
        .map_err(|_| deadline_elapsed())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UnreachableTransport;

    #[async_trait]
    impl JdVopReadTransport for UnreachableTransport {
        async fn read(&self, _: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
            panic!("closed admission must not send a read");
        }
    }

    #[async_trait]
    impl JdVopWriteTransport for UnreachableTransport {
        async fn write(&self, _: &JdVopWriteRequest) -> ConnectorResult<Vec<u8>> {
            panic!("closed admission must not send a write");
        }
    }

    #[tokio::test]
    async fn closed_shared_admission_preserves_read_and_write_error_classes() {
        let transport = Arc::new(UnreachableTransport);
        let reader = Arc::new(ReadClient::new(transport.clone(), 1, 1, 1));
        reader.semaphore.close();
        let error = reader.read::<WireText>(JdVopReadRequest::Balance).await.err().unwrap();
        assert_eq!(error.class, SupplierFailureClass::TransientFailure);
        assert_eq!(error.code, "JD_VOP_READ_CLOSED");

        let writer = WriteClient { transport, reader };
        let error = writer
            .send_true(JdVopWriteRequest::DeleteMessages { ids: vec!["123".into()], source_type: 1 })
            .await
            .unwrap_err();
        assert_eq!(error.class, SupplierFailureClass::ResultUnknown);
        assert_eq!(error.code, "JD_VOP_WRITE_RESULT_UNKNOWN");
    }
}
