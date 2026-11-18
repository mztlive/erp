//! 回调接收的持久化边界；协议判断和重复证据核验由接收编排负责。

use async_trait::async_trait;
use erp_integration::entity::supplier_callback::SupplierCallbackReceipt;
use erp_integration::repository::SupplierCallbackExt;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use persistence_core::{NoTransaction, Result as StoreResult};

use crate::Result;

/// 为一次回调提供服务端连接和原回执，禁止以接收正文替代连接事实。
#[async_trait]
pub trait CallbackReceiptStore: Send + Sync {
    /// 加载服务端登记的原连接。
    ///
    /// `id` 为路由中的连接身份。
    ///
    /// 原连接及当前技术绑定。
    ///
    /// 连接不存在或读取失败时返回原错误。
    async fn connection(&self, id: &str) -> Result<SupplierApiConnection>;

    /// 可靠保存已经验签并加密的原回执。
    ///
    /// `receipt` 为接收编排构造的完整证据。
    ///
    /// 保存成功后返回。
    ///
    /// 重复键及未知写入结果保留原持久化分类，交由接收编排解释。
    async fn create(&self, receipt: &SupplierCallbackReceipt) -> StoreResult<()>;

    /// 按同一回调身份读取既有证据。
    ///
    /// `id` 为本次接收计算出的原回执身份。
    ///
    /// 已持久化回执或不存在。
    ///
    /// 读取失败时返回原错误，不生成替代身份。
    async fn find(&self, id: &str) -> StoreResult<Option<SupplierCallbackReceipt>>;
}

pub(super) struct MongoCallbackReceipts<'a>(pub &'a Database);

#[async_trait]
impl CallbackReceiptStore for MongoCallbackReceipts<'_> {
    async fn connection(&self, id: &str) -> Result<SupplierApiConnection> {
        Ok(SupplierApiService::new(self.0.clone()).load_connection(id, &mut NoTransaction).await?)
    }

    async fn create(&self, receipt: &SupplierCallbackReceipt) -> StoreResult<()> {
        self.0.supplier_callback_receipts().create(receipt, &mut NoTransaction).await
    }

    async fn find(&self, id: &str) -> StoreResult<Option<SupplierCallbackReceipt>> {
        self.0.supplier_callback_receipts().find_by_id(id, &mut NoTransaction).await
    }
}
