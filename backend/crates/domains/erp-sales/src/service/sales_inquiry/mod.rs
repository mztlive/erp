//! 询价本域查询及可加入 Process 同一事务的持久化步骤。
use mongodb::Database;
use persistence_core::{Error as PersistenceError, Executor, PageResult};

use crate::dto::sales_inquiry::{InquiryProcurementView, InquirySalesView};
use crate::entity::sales_inquiry::{InquiryCommandReceipt, InquiryCreateData, SalesInquiry};
use crate::repository::SalesInquiryExt;
use crate::repository::sales_inquiry::{InquiryListFilter, SalesInquiryRepository};
use crate::{Error, Result};

pub struct SalesInquiryService {
    db: Database,
}

impl SalesInquiryService {
    /// 创建仅持有本域数据库的服务。
    /// # 参数
    /// `db` 为询价数据库。
    /// # 返回
    /// 返回服务实例。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 读取当前询价，访问资格由调用方用同一执行器重验。
    /// # 参数
    /// `id` 为稳定询价身份；`executor` 为数据访问执行器。
    /// # 返回
    /// 返回完整本域聚合，禁止直接作为 HTTP 销售响应。
    /// # 错误
    /// 不存在或存储失败时拒绝。
    pub async fn require(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesInquiry> {
        self.db
            .sales_inquiries()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("询价".into()))
    }

    /// 在调用方事务中创建草稿。
    /// # 参数
    /// `id`、`inquiry_no` 为服务端身份；`data` 为创建资料；`executor` 为同一事务。
    /// # 返回
    /// 返回已写入的草稿。
    /// # 错误
    /// 领域校验或唯一索引不满足时拒绝。
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

    /// 使用聚合版本 CAS 保存领域步骤结果。
    /// # 参数
    /// `inquiry` 为当前执行器内读取后修改的聚合；`executor` 为同一事务。
    /// # 返回
    /// 成功后同步递增内存版本。
    /// # 错误
    /// 版本不匹配或持久化失败时拒绝；不得重试不明提交。
    pub async fn save(&self, inquiry: &mut SalesInquiry, executor: &mut dyn Executor) -> Result<()> {
        SalesInquiryRepository::new(&self.db).update(inquiry, executor).await?;
        Ok(())
    }

    /// 分页读取已授权范围，默认筛选拒绝全部。
    /// # 参数
    /// `filter` 为已授权筛选；`executor` 为执行器。
    /// # 返回
    /// 返回稳定分页聚合。
    /// # 错误
    /// 数据访问失败时拒绝。
    pub async fn inquiry_list(
        &self,
        filter: &InquiryListFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesInquiry>> {
        Ok(SalesInquiryRepository::new(&self.db).inquiry_list(filter, executor).await?)
    }

    /// 取得不含采购工作草稿及内部资料的销售投影。
    /// # 参数
    /// `id` 为已授权询价；`executor` 为执行器。
    /// # 返回
    /// 返回独立销售 DTO。
    /// # 错误
    /// 询价不存在或存储失败时拒绝。
    pub async fn sales_view(&self, id: &str, executor: &mut dyn Executor) -> Result<InquirySalesView> {
        Ok(InquirySalesView::from_inquiry(&self.require(id, executor).await?))
    }

    /// 取得不含销售参考价的采购投影；任务入口应使用 from_scoped。
    /// # 参数
    /// `id` 为已授权询价；`executor` 为执行器。
    /// # 返回
    /// 返回独立采购 DTO。
    /// # 错误
    /// 询价不存在或存储失败时拒绝。
    pub async fn procurement_view(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<InquiryProcurementView> {
        Ok(InquiryProcurementView::from_inquiry(&self.require(id, executor).await?))
    }

    /// 读取独立命令回执，恢复后仍须重新执行当前授权。
    /// # 参数
    /// `id` 为从调用人、动作和幂等键生成的命令身份。
    /// # 返回
    /// 返回存在的原成功回执。
    /// # 错误
    /// 存储失败时拒绝。
    pub async fn receipt(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<InquiryCommandReceipt>> {
        let receipt = self.db.inquiry_command_receipts().find_by_id_including_deleted(id, executor).await?;
        receipt.map(|receipt| receipt.validate().map(|()| receipt)).transpose()
    }

    /// 同事务追加成功命令回执，创建命令同样可恢复。
    /// # 参数
    /// `receipt` 为已核对命令与结果的强类型回执。
    /// # 返回
    /// 成功时返回空结果。
    /// # 错误
    /// 重复身份或存储失败时拒绝，调用方回滚全部业务步骤。
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

    /// 按正式办理单据反查费用来源，禁止客户端声明已办理事实。
    /// # 参数
    /// `source_document_id` 为采购或财务单据；`executor` 为同一事务。
    /// # 返回
    /// 返回最多 50 份关联询价。
    /// # 错误
    /// 存储失败时拒绝。
    pub async fn fee_sources(
        &self,
        source_document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInquiry>> {
        Ok(SalesInquiryRepository::new(&self.db).fee_sources(source_document_id, executor).await?)
    }
}
