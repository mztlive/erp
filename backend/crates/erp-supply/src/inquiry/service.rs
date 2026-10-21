//! 在调用方事务中保存及精确复核内部询价报价。
use entity_core::BaseModel;
use erp_core::common::time::BusinessDate;
use erp_core::money::Quantity;
use mongodb::Database;
use persistence_core::Executor;

use super::{
    InquiryFeeBasis, InquiryFeeBasisRepositoryExt, InquiryQuote, InquiryQuoteBalance,
    InquiryQuoteRepositoryExt, InquiryQuoteReservation, InquiryQuoteReservationRepositoryExt,
    InquiryQuoteUse, InquirySupplyExt, QuoteAddressLimit,
};
use crate::entity::supplier_offering::OfferingStatus;
use crate::ports::offering_qualification::QualificationPort;
use crate::repository::SupplierOfferingExt;
use crate::{Error, Result};

/// 同一正式采购采用的精确版本与稳定采购身份；仅由Process构造。
pub struct QuoteReservationInput<'a> {
    pub quote_revision_id: &'a str,
    pub quote_version: u64,
    pub reservation_id: &'a str,
    pub purchase_order_id: &'a str,
    pub usage: &'a InquiryQuoteUse,
}

/// 内部报价由 Process 校验当前任务、责任范围、版本与附件关系后调用。
#[derive(Clone)]
pub struct InquirySupplyService {
    db: Database,
}
impl InquirySupplyService {
    /// 装配内部报价服务。
    /// # 参数
    /// `db` 为供应领域数据库。
    /// # 返回
    /// 返回服务实例。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 读取精确询价需求版本的内部报价历史，不读取通用推荐。
    /// # 参数
    /// `inquiry_id`、`requirement_version` 为已授权需求；`executor` 为调用方事务。
    /// # 返回
    /// 返回该需求版本的内部报价。
    /// # 错误
    /// 数据读取失败时返回错误。
    pub async fn quotes(
        &self,
        inquiry_id: &str,
        requirement_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuote>> {
        self.db
            .inquiry_quotes()
            .for_requirement(inquiry_id, requirement_version, executor)
            .await
            .map_err(Into::into)
    }

    /// 读取指定的不可变报价版本。
    /// # 参数
    /// `id` 为报价版本主键；`executor` 为调用方执行器。
    /// # 返回
    /// 返回内部报价；缺失或删除时拒绝。
    /// # 错误
    /// 找不到报价或读取失败时返回错误。
    pub async fn quote(&self, id: &str, executor: &mut dyn Executor) -> Result<InquiryQuote> {
        self.db
            .inquiry_quotes()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("询价内部报价不存在".into()))
    }

    /// 追加不可变报价，不隐式维护通用供给条款。
    /// # 参数
    /// `quote` 为规范化报价；`port` 提供供应资格；`executor` 为当前写事务。
    /// # 返回
    /// 保存报价成功。
    /// # 错误
    /// 版本不连续、稳定身份换范围、供给归属或资格失效时拒绝。
    pub async fn append_quote<P: QualificationPort + ?Sized>(
        &self,
        quote: &InquiryQuote,
        port: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<(), P::Error> {
        let quote = InquiryQuote::new(quote.base.id.clone(), quote.data.clone())?;
        let prior = self.db.inquiry_quotes().history(&quote.data.quote_id, executor).await?;
        quote.ensure_next_revision(prior.first())?;
        if prior.is_empty() {
            self.db
                .inquiry_quote_balances()
                .create(&InquiryQuoteBalance::empty(quote.data.quote_id.clone())?, executor)
                .await?;
        }
        self.ensure_offering(&quote, executor).await?;
        port.ensure_qualified(&quote.data.supplier_id, &quote.data.sku_id, BusinessDate::today(), executor)
            .await?;
        self.db.inquiry_quotes().create(&quote, executor).await?;
        Ok(())
    }

    /// 在正式采购事务精确复验报价版本、作用域、资格及商业条件。
    /// # 参数
    /// `id`、`expected_version` 为候选冻结版本；`usage` 为当前采用事实，`port` 重验资格。
    /// # 返回
    /// 返回原始不可变报价，不替换任何价格。
    /// # 错误
    /// 原报价变化、过期、越界或资格失效时拒绝。
    pub async fn ensure_quote<P: QualificationPort + ?Sized>(
        &self,
        id: &str,
        expected_version: u64,
        usage: &InquiryQuoteUse,
        port: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<InquiryQuote, P::Error> {
        let quote = self.quote(id, executor).await?;
        if quote.base.version != expected_version {
            return Err(Error::ConflictError("专项报价版本已变化，请重新确认".into()).into());
        }
        quote.ensure_applicable(usage)?;
        self.ensure_offering(&quote, executor).await?;
        port.ensure_qualified(&quote.data.supplier_id, &quote.data.sku_id, usage.on_date, executor).await?;
        Ok(quote)
    }

    /// 追加当前反馈精确费用的采购内部依据。
    /// # 参数
    /// `basis` 为权限及当前责任已校验的内部收款依据；`executor` 为当前事务。
    /// # 返回
    /// 保存成功；同一反馈稳定费用不得重复保存。
    /// # 错误
    /// 依据非法、重复或持久化失败时拒绝。
    pub async fn append_fee_basis(&self, basis: &InquiryFeeBasis, executor: &mut dyn Executor) -> Result<()> {
        let basis = InquiryFeeBasis::new(basis.base.id.clone(), basis.data.clone())?;
        self.db.inquiry_fee_bases().create(&basis, executor).await.map_err(Into::into)
    }

    /// 读取准确费用的内部依据；已发布轮次的写入资格由Process拒绝。
    /// # 参数
    /// 精确询价、需求、反馈、费用身份及当前执行器。
    /// # 返回
    /// 返回原内部依据及稳定附件归属ID。
    /// # 错误
    /// 读取失败时返回错误。
    pub async fn fee_basis(
        &self,
        inquiry_id: &str,
        version: u32,
        round: u32,
        fee_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InquiryFeeBasis>> {
        self.db
            .inquiry_fee_bases()
            .for_fee(inquiry_id, version, round, fee_id, executor)
            .await
            .map_err(Into::into)
    }
    /// 保存尚未发布轮次的费用内部工作依据，保持稳定附件归属。
    /// # 参数
    /// `basis`为Process已验证未发布轮次的输入，`executor`为含任务/聚合CAS的事务。
    /// # 返回
    /// 新增或同ID更新成功，不修改其他轮次。
    /// # 错误
    /// 内容非法、稳定身份不符或并发CAS失败时拒绝。
    pub async fn save_fee_basis(&self, basis: &InquiryFeeBasis, executor: &mut dyn Executor) -> Result<()> {
        let normalized = InquiryFeeBasis::new(basis.base.id.clone(), basis.data.clone())?;
        let data = &normalized.data;
        let existing = self
            .fee_basis(
                &data.inquiry_id,
                data.requirement_version,
                data.feedback_round,
                &data.fee_id,
                executor,
            )
            .await?;
        if let Some(mut existing) = existing {
            if existing.base.id != normalized.base.id {
                return Err(Error::ConflictError("费用内部依据稳定身份不符".into()));
            }
            existing.data = normalized.data;
            self.db.inquiry_fee_bases().update(&mut existing, executor).await?;
        } else {
            self.db.inquiry_fee_bases().create(&normalized, executor).await?;
        }
        Ok(())
    }

    /// 读取明确询价及反馈轮次的费用收款依据。
    /// # 参数
    /// 精确询价、需求版本、反馈轮次和当前调用方执行器。
    /// # 返回
    /// 返回该轮内部费用依据，不对普通销售视图返回。
    /// # 错误
    /// 读取失败时返回错误。
    pub async fn fee_bases(
        &self,
        inquiry_id: &str,
        requirement_version: u32,
        feedback_round: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryFeeBasis>> {
        self.db
            .inquiry_fee_bases()
            .for_feedback(inquiry_id, requirement_version, feedback_round, executor)
            .await
            .map_err(Into::into)
    }

    /// 与正式采购原子登记专项报价采用，并用稳定报价余额CAS阻止跨销售超量。
    /// # 参数
    /// `id`、`version` 为原报价；`reservation_id` 为采购行ID；`usage` 为本次真实承担量。
    /// # 返回
    /// 精确采用的原始报价。
    /// # 错误
    /// 条件失效、余额不足、重复采用或并发CAS失败时拒绝。
    pub async fn reserve_quote<P: QualificationPort + ?Sized>(
        &self,
        input: QuoteReservationInput<'_>,
        port: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<InquiryQuote, P::Error> {
        let QuoteReservationInput {
            quote_revision_id: id,
            quote_version: version,
            reservation_id,
            purchase_order_id,
            usage,
        } = input;
        let quote = self.ensure_quote(id, version, usage, port, executor).await?;
        let mut balance = self
            .db
            .inquiry_quote_balances()
            .find_by_id(&quote.data.quote_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("专项报价余额缺失，禁止采用".into()))?;
        balance.reserve(&quote, usage)?;
        self.db.inquiry_quote_balances().update(&mut balance, executor).await?;
        let reservation = InquiryQuoteReservation {
            base: BaseModel::new(reservation_id.to_string()),
            purchase_order_id: purchase_order_id.to_string(),
            released: false,
            quote_id: quote.data.quote_id.clone(),
            quote_revision_id: quote.base.id.clone(),
            quantity: usage.quantity,
            addresses: usage.addresses.clone(),
            history: vec![],
        };
        self.db.inquiry_quote_reservations().create(&reservation, executor).await?;
        Ok(quote)
    }

    /// 重验并调整同一采购采用；审批期间不提前释放原正式需求。
    /// # 参数
    /// `reservation_id`为原采用，`usage`为服务器新目标，`keep_existing`为审批中保留旧范围。
    /// # 返回
    /// 新余额与采用历史原子更新，调用方负责整个采购事务。
    /// # 错误
    /// 报价或资格失效、余额不足和CAS冲突时拒绝。
    pub async fn resize_quote<P: QualificationPort + ?Sized>(
        &self,
        reservation_id: &str,
        usage: &InquiryQuoteUse,
        keep_existing: bool,
        port: &P,
        executor: &mut dyn Executor,
    ) -> std::result::Result<InquiryQuote, P::Error> {
        let mut reservation = self
            .db
            .inquiry_quote_reservations()
            .find_by_id(reservation_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("专项采购采用记录缺失".into()))?;
        if reservation.released {
            return Err(Error::ConflictError("专项采购采用已释放".into()).into());
        }
        let quote = self.quote(&reservation.quote_revision_id, executor).await?;
        quote.ensure_applicable(usage)?;
        self.ensure_offering(&quote, executor).await?;
        port.ensure_qualified(&quote.data.supplier_id, &quote.data.sku_id, usage.on_date, executor).await?;
        let mut balance = self
            .db
            .inquiry_quote_balances()
            .find_by_id(&quote.data.quote_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("专项报价余额缺失".into()))?;
        reservation = balance.resized(&quote, usage, &reservation, keep_existing)?;
        self.db.inquiry_quote_balances().update(&mut balance, executor).await?;
        self.db.inquiry_quote_reservations().update(&mut reservation, executor).await?;
        Ok(quote)
    }

    /// 根据已保存正式采购事实恢复原采用，仅缩减待审保留量。
    /// # 参数
    /// 原采用ID、原专项报价版本、正式数量与地址及调用方作废事务。
    /// # 返回
    /// 返回原报价历史；不检查当前供应商资格和报价期限。
    /// # 错误
    /// 采用缺失、已释放、换报价、增量、新地址或CAS冲突时拒绝。
    pub async fn restore_existing_usage(
        &self,
        reservation_id: &str,
        quote_revision_id: &str,
        quantity: Quantity,
        addresses: &[QuoteAddressLimit],
        executor: &mut dyn Executor,
    ) -> Result<InquiryQuote> {
        let mut reservation = self
            .db
            .inquiry_quote_reservations()
            .find_by_id(reservation_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("原专项采购采用缺失，禁止恢复".into()))?;
        if reservation.quote_revision_id != quote_revision_id {
            return Err(Error::ConflictError("恢复不得切换原专项报价".into()));
        }
        let quote = self.quote(quote_revision_id, executor).await?;
        if quote.data.quote_id != reservation.quote_id {
            return Err(Error::ConflictError("原专项报价与采用余额归属不一致".into()));
        }
        let mut balance = self
            .db
            .inquiry_quote_balances()
            .find_by_id(&reservation.quote_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("原专项报价余额缺失".into()))?;
        reservation = balance.restore_existing_usage(&reservation, quantity, addresses)?;
        self.db.inquiry_quote_balances().update(&mut balance, executor).await?;
        self.db.inquiry_quote_reservations().update(&mut reservation, executor).await?;
        Ok(quote)
    }

    /// 查询具体采购的专项采用，不提供任意过滤器。
    /// # 参数
    /// `order_id`为已证明范围的采购，`executor`为原事务。
    /// # 返回
    /// 返回未释放采用。
    /// # 错误
    /// 读取失败时返回错误。
    pub async fn reservations(
        &self,
        order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<InquiryQuoteReservation>> {
        self.db.inquiry_quote_reservations().active_for_order(order_id, executor).await.map_err(Into::into)
    }
    /// 按正式减少或作废事实释放一条准确采用。
    /// # 参数
    /// `reservation_id`为原采购采用身份，`executor`为原业务事务。
    /// # 返回
    /// 保留历史数量并标记已释放，重复调用无变化。
    /// # 错误
    /// 余额损坏、并发修改或读取失败时拒绝。
    pub async fn release_reservation(&self, reservation_id: &str, executor: &mut dyn Executor) -> Result<()> {
        let mut reservation = self
            .db
            .inquiry_quote_reservations()
            .find_by_id(reservation_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("专项采购采用缺失".into()))?;
        if reservation.released {
            return Ok(());
        }
        let mut balance = self
            .db
            .inquiry_quote_balances()
            .find_by_id(&reservation.quote_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("专项报价采用余额缺失".into()))?;
        balance.release(&reservation)?;
        self.db.inquiry_quote_balances().update(&mut balance, executor).await?;
        reservation.released = true;
        self.db.inquiry_quote_reservations().update(&mut reservation, executor).await?;
        Ok(())
    }

    /// 按采购正式作废资格释放专项采用；调用方先校验收货、履约和财务事实。
    /// # 参数
    /// `purchase_order_id`为正式允许作废的采购单，`executor`为原作废事务。
    /// # 返回
    /// 保留原采用数量并标记已释放，重复调用无额外变化。
    /// # 错误
    /// 余额损坏、并发更新或持久化失败时拒绝。
    pub async fn release_order_quotes(
        &self,
        purchase_order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for reservation in self.reservations(purchase_order_id, executor).await? {
            self.release_reservation(&reservation.base.id, executor).await?;
        }
        Ok(())
    }

    async fn ensure_offering(&self, quote: &InquiryQuote, executor: &mut dyn Executor) -> Result<()> {
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(quote.data.supplier_offering_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("询价报价引用供给不存在".into()))?;
        if offering.supplier_id != quote.data.supplier_id
            || offering.sku_id != quote.data.sku_id
            || offering.stable.status != OfferingStatus::Active
        {
            return Err(Error::BusinessLogicError("询价报价供给归属不符或已停用".into()));
        }
        Ok(())
    }
}
