//! 询价本域查询及可加入 Process 同一事务的持久化步骤。
use mongodb::Database;
use persistence_core::{Error as PersistenceError, Executor, PageResult};

use crate::dto::sales_inquiry::{InquiryProcurementView, InquirySalesView};
use crate::entity::sales_inquiry::{InquiryCommandReceipt, InquiryCreateData, SalesInquiry};
use crate::repository::SalesInquiryExt;
use crate::repository::sales_inquiry::{InquiryListFilter, InquiryReadScope, SalesInquiryRepository};
use crate::{Error, Result};

pub struct SalesInquiryService {
    db: Database,
}

impl SalesInquiryService {
    /// 在组装询价用例时绑定本域数据库句柄，实际查询或写入由各用例方法执行。
    ///
    /// 输入约定：`db` 为询价数据库。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 询价命令或详情需要聚合时按身份加载，调用方必须在同一执行器复验访问资格。
    /// 返回完整本域聚合，禁止直接作为 HTTP 销售响应。
    /// 不存在或存储失败时拒绝。
    ///
    /// 输入约定：`id` 为稳定询价身份；`executor` 为数据访问执行器。
    pub async fn require(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesInquiry> {
        self.db
            .sales_inquiries()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("询价".into()))
    }

    /// 销售新建询价草稿时将已构造聚合插入调用方事务；不在此自行开事务。
    /// 返回已写入的草稿。
    /// 领域校验或唯一索引不满足时拒绝。
    ///
    /// 输入约定：`id`、`inquiry_no` 为服务端身份；`data` 为创建资料；`executor` 为同一事务。
    pub async fn create(
        &self,
        id: String,
        inquiry_no: String,
        data: InquiryCreateData,
        executor: &mut dyn Executor,
    ) -> Result<SalesInquiry> {
        let inquiry = SalesInquiry::create(id, inquiry_no, data)?;
        SalesInquiryRepository::new(&self.db).create(&inquiry, executor).await?;
        Ok(inquiry)
    }

    /// 领域步骤完成后按预期聚合版本持久化询价，拒绝用旧副本覆盖并发写入。
    /// 成功后同步递增内存版本。
    /// 失败条件：版本不匹配或持久化失败时拒绝；不得重试不明提交。
    ///
    /// 输入约定：`inquiry` 为当前执行器内读取后修改的聚合；`executor` 为同一事务。
    pub async fn save(&self, inquiry: &mut SalesInquiry, executor: &mut dyn Executor) -> Result<()> {
        SalesInquiryRepository::new(&self.db).update(inquiry, executor).await?;
        Ok(())
    }

    /// 在授权范围内分页读取询价；没有显式授权范围时使用默认拒绝条件，而非查全量。
    /// 返回稳定分页聚合。
    /// 数据访问失败时拒绝。
    ///
    /// 输入约定：`filter` 为已授权筛选；`executor` 为执行器。
    pub async fn inquiry_list(
        &self,
        filter: &InquiryListFilter,
        access_scope: &InquiryReadScope,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesInquiry>> {
        SalesInquiryRepository::new(&self.db).inquiry_list(filter, access_scope, executor).await
    }

    /// 向销售返回已公开需求、反馈和办理进度，不投影采购未发布工作或内部字段。
    /// 返回独立销售 DTO。
    /// 询价不存在或存储失败时拒绝。
    ///
    /// 输入约定：`id` 为已授权询价；`executor` 为执行器。
    pub async fn sales_view(&self, id: &str, executor: &mut dyn Executor) -> Result<InquirySalesView> {
        Ok(InquirySalesView::from_inquiry(&self.require(id, executor).await?))
    }

    /// 为获准处理该询价的采购返回工作详情，不把销售参考价带入采购视图。
    /// 返回独立采购 DTO。
    /// 询价不存在或存储失败时拒绝。
    ///
    /// 输入约定：`id` 为已授权询价；`executor` 为执行器。
    pub async fn procurement_view(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<InquiryProcurementView> {
        Ok(InquiryProcurementView::from_inquiry(&self.require(id, executor).await?))
    }

    /// 询价命令重试时按命令身份读取独立的成功回执，恢复原结果前仍须重验当前授权。
    /// 返回存在的原成功回执。
    /// 存储失败时拒绝。
    ///
    /// 输入约定：`id` 为从调用人、动作和幂等键生成的命令身份。
    pub async fn receipt(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InquiryCommandReceipt>> {
        let receipt = self.db.inquiry_command_receipts().find_by_id_including_deleted(id, executor).await?;
        receipt.map(|receipt| receipt.validate().map(|()| receipt)).transpose()
    }

    /// 首次创建或处理询价成功时，用同一事务追加独立回执，使重试可定位原结果而不重复办理。
    /// 失败条件：重复身份或存储失败时拒绝，调用方回滚全部业务步骤。
    ///
    /// 输入约定：`receipt` 为已核对命令与结果的强类型回执。
    pub async fn save_receipt(
        &self,
        receipt: &InquiryCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        receipt.validate()?;
        self.db.inquiry_command_receipts().create(receipt, executor).await.map_err(|error| match error {
            duplicate @ PersistenceError::DuplicateKey(_) => Error::ReceiptDuplicate(duplicate),
            other => Error::from(other),
        })
    }

    /// 下游需要核验费用来源时从正式办理单据反查，用持久化引用而不是客户端声明证明已办理事实。
    /// 返回最多 50 份关联询价。
    /// 存储失败时拒绝。
    ///
    /// 输入约定：`source_document_id` 为采购或财务单据；`executor` 为同一事务。
    pub async fn fee_sources(
        &self,
        source_document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInquiry>> {
        Ok(SalesInquiryRepository::new(&self.db).fee_sources(source_document_id, executor).await?)
    }
}
