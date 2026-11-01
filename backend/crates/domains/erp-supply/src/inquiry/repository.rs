//! 内部询价报价与费用依据的独立集合访问器。
use mongodb::Database;
use persistence_core::Repository;

use super::{InquiryFeeBasis, InquiryQuote, InquiryQuoteBalance, InquiryQuoteReservation};

/// 本领域拥有的内部询价集合；销售视图不得读取这些集合。
pub trait InquirySupplyExt {
    const INQUIRY_QUOTE_BALANCES: &'static str = "sales_inquiry_quote_balances";
    const INQUIRY_QUOTE_RESERVATIONS: &'static str = "sales_inquiry_quote_reservations";
    const INQUIRY_QUOTES: &'static str = "sales_inquiry_internal_quotes";
    const INQUIRY_FEE_BASES: &'static str = "sales_inquiry_internal_fee_bases";
    /// 取得稳定报价累计余额仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 同一事务内CAS更新的余额仓储。
    /// # 错误
    /// 无。
    fn inquiry_quote_balances(&self) -> Repository<'_, InquiryQuoteBalance>;
    /// 取得具体采购行的不可变采用记录。
    /// # 参数
    /// 无。
    /// # 返回
    /// 采用记录仓储。
    /// # 错误
    /// 无。
    fn inquiry_quote_reservations(&self) -> Repository<'_, InquiryQuoteReservation>;
    /// 取得不可变报价仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 调用方执行器驱动的报价仓储。
    /// # 错误
    /// 无。
    fn inquiry_quotes(&self) -> Repository<'_, InquiryQuote>;
    /// 取得费用内部依据仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 调用方执行器驱动的费用仓储。
    /// # 错误
    /// 无。
    fn inquiry_fee_bases(&self) -> Repository<'_, InquiryFeeBasis>;
}
impl InquirySupplyExt for Database {
    fn inquiry_quote_balances(&self) -> Repository<'_, InquiryQuoteBalance> {
        Repository::new(self, Self::INQUIRY_QUOTE_BALANCES)
    }
    fn inquiry_quote_reservations(&self) -> Repository<'_, InquiryQuoteReservation> {
        Repository::new(self, Self::INQUIRY_QUOTE_RESERVATIONS)
    }
    fn inquiry_quotes(&self) -> Repository<'_, InquiryQuote> {
        Repository::new(self, Self::INQUIRY_QUOTES)
    }
    fn inquiry_fee_bases(&self) -> Repository<'_, InquiryFeeBasis> {
        Repository::new(self, Self::INQUIRY_FEE_BASES)
    }
}

use async_trait::async_trait;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

/// 内部报价只提供精确询价范围和稳定报价版本查询。
#[async_trait]
pub trait InquiryQuoteRepositoryExt {
    /// 读取指定需求版本的报价历史。
    /// # 参数
    /// 询价、需求版本和调用方执行器。
    /// # 返回
    /// 按需求行及稳定报价版本排序的内部报价。
    /// # 错误
    /// 数据库查询或反序列化失败时拒绝。
    async fn for_requirement(
        &self,
        inquiry_id: &str,
        version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuote>>;
    /// 读取已接受反馈准确轮次的报价历史。
    /// # 参数
    /// 询价、需求版本、反馈轮次及执行器。
    /// # 返回
    /// 只属于本轮次的报价历史。
    /// # 错误
    /// 数据库查询或反序列化失败时拒绝。
    async fn for_feedback(
        &self,
        inquiry_id: &str,
        version: u32,
        round: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuote>>;
    /// 读取一个稳定报价的修订历史。
    /// # 参数
    /// 稳定报价ID及当前事务执行器。
    /// # 返回
    /// 最新修订在前的不可变历史。
    /// # 错误
    /// 数据库读取失败时拒绝。
    async fn history(&self, quote_id: &str, executor: &mut dyn Executor) -> Result<Vec<InquiryQuote>>;
}
#[async_trait]
impl InquiryQuoteRepositoryExt for Repository<'_, InquiryQuote> {
    async fn for_requirement(
        &self,
        inquiry_id: &str,
        version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuote>> {
        self.find_many_sorted(
            doc! {"inquiry_id":inquiry_id,"requirement_version":i64::from(version)},
            doc! {"line_id":1,"quote_id":1,"revision_no":1},
            executor,
        )
        .await
    }
    async fn for_feedback(
        &self,
        inquiry_id: &str,
        version: u32,
        round: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuote>> {
        self.find_many_sorted(doc! {"inquiry_id":inquiry_id,"requirement_version":i64::from(version),"feedback_round":i64::from(round)},
            doc! {"quote_id":1,"revision_no":1},executor).await
    }
    async fn history(&self, quote_id: &str, executor: &mut dyn Executor) -> Result<Vec<InquiryQuote>> {
        self.find_many_sorted(doc! {"quote_id":quote_id}, doc! {"revision_no":-1}, executor).await
    }
}
/// 费用内部依据查询范围由固定字段决定，禁止暴露任意过滤器。
#[async_trait]
pub trait InquiryFeeBasisRepositoryExt {
    /// 读取准确费用的内部依据。
    /// # 参数
    /// 询价、需求版本、反馈轮次、稳定费用身份及执行器。
    /// # 返回
    /// 返回精确费用依据；缺失时为None。
    /// # 错误
    /// 数据读取失败时返回错误。
    async fn for_fee(
        &self,
        inquiry_id: &str,
        version: u32,
        round: u32,
        fee_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InquiryFeeBasis>>;
    /// 读取本轮费用内部依据。
    /// # 参数
    /// 精确询价、需求、反馈轮次及执行器。
    /// # 返回
    /// 当前轮次费用的收款依据。
    /// # 错误
    /// 数据读取失败时返回错误。
    async fn for_feedback(
        &self,
        inquiry_id: &str,
        version: u32,
        round: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryFeeBasis>>;
}
#[async_trait]
impl InquiryFeeBasisRepositoryExt for Repository<'_, InquiryFeeBasis> {
    async fn for_fee(
        &self,
        inquiry_id: &str,
        version: u32,
        round: u32,
        fee_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InquiryFeeBasis>> {
        self.find_one(doc! {"inquiry_id":inquiry_id,"requirement_version":i64::from(version),"feedback_round":i64::from(round),"fee_id":fee_id},executor).await
    }
    async fn for_feedback(
        &self,
        inquiry_id: &str,
        version: u32,
        round: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryFeeBasis>> {
        self.find_many(doc! {"inquiry_id":inquiry_id,"requirement_version":i64::from(version),"feedback_round":i64::from(round)},executor).await
    }
}
/// 供应采用固定按已授权采购单读取。
#[async_trait]
pub trait InquiryQuoteReservationRepositoryExt {
    /// 读取采购未释放的专项采用。
    /// # 参数
    /// 已授权采购单与调用方执行器。
    /// # 返回
    /// 当前有效采用记录。
    /// # 错误
    /// 数据读取失败时返回错误。
    async fn active_for_order(
        &self,
        order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuoteReservation>>;
}
#[async_trait]
impl InquiryQuoteReservationRepositoryExt for Repository<'_, InquiryQuoteReservation> {
    async fn active_for_order(
        &self,
        order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuoteReservation>> {
        self.find_many(doc! {"purchase_order_id":order_id,"released":false}, executor).await
    }
}
/// 报价余额批量查询只接受已核验的稳定报价集合。
#[async_trait]
pub trait InquiryQuoteBalanceRepositoryExt {
    /// 批量读取稳定报价采用余额。
    /// # 参数
    /// 稳定报价集合及调用方执行器。
    /// # 返回
    /// 已存在的报价余额；缺失不伪造零余额。
    /// # 错误
    /// 数据读取失败时返回错误。
    async fn for_quotes(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuoteBalance>>;
}
#[async_trait]
impl InquiryQuoteBalanceRepositoryExt for Repository<'_, InquiryQuoteBalance> {
    async fn for_quotes(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuoteBalance>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        self.find_many(doc! {"id":{"$in":ids}}, executor).await
    }
}
