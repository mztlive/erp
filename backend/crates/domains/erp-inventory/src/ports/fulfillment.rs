//! 流水视图使用的采购入库单号消费端口。

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Minimal purchase-receipt identity used to hydrate movement source document numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptNoFact {
    /// Purchase receipt id.
    pub id: String,
    /// Receipt number.
    pub receipt_no: String,
}

/// 库存用来读取入库单号、且不依赖履约领域的端口。
#[async_trait]
pub trait FulfillmentFactsPort: Send + Sync {
    /// 按采购入库单标识返回单号。
    ///
    /// # 参数
    /// * `ids` - 采购入库单标识。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 返回以入库单标识为键的单号事实映射。
    ///
    /// # 错误
    /// 适配器查询失败时返回对应错误。
    async fn receipt_nos_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReceiptNoFact>>;
}

/// Fail-closed fulfillment facts port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFulfillmentFacts;

#[async_trait]
impl FulfillmentFactsPort for FailClosedFulfillmentFacts {
    async fn receipt_nos_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReceiptNoFact>> {
        Err(Error::Internal("履约入库事实端口未接线".to_string()))
    }
}
