//! 不可变付款申请提交快照仓储，只暴露插入和精确关联读取。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::payable::PurchasePaymentRequestSubmissionSnapshot;

/// 付款申请提交快照集合访问器。
pub trait PurchasePaymentRequestSnapshotExt {
    const PURCHASE_PAYMENT_REQUEST_SNAPSHOTS: &'static str = "purchase_payment_request_snapshots";

    /// 获取只允许插入和读取的提交快照仓储。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回申请提交快照的窄仓储。
    /// # 错误
    /// 不返回错误。
    fn purchase_payment_request_snapshots(&self) -> PurchasePaymentRequestSnapshotRepository<'_>;
}

impl PurchasePaymentRequestSnapshotExt for Database {
    fn purchase_payment_request_snapshots(&self) -> PurchasePaymentRequestSnapshotRepository<'_> {
        PurchasePaymentRequestSnapshotRepository {
            repository: Repository::new(self, Self::PURCHASE_PAYMENT_REQUEST_SNAPSHOTS),
        }
    }
}

/// 不公开更新、删除或集合访问，提交材料禁止被后续申请改写。
pub struct PurchasePaymentRequestSnapshotRepository<'a> {
    repository: Repository<'a, PurchasePaymentRequestSubmissionSnapshot>,
}

impl PurchasePaymentRequestSnapshotRepository<'_> {
    /// 沿不可变采购来源查找历史申请，防止重新提交后丢失关联。
    ///
    /// # 参数
    /// `purchase_ids` 为已定位的采购单；`executor` 为原读取事务。
    /// # 返回
    /// 返回包含这些采购来源的历史提交快照，空输入返回空列表。
    /// # 错误
    /// 数据库或反序列化失败时返回错误。
    pub async fn find_for_purchases(
        &self,
        purchase_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchasePaymentRequestSubmissionSnapshot>> {
        if purchase_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.repository
            .find_many(doc! { "request.lines.purchase_order_id": { "$in": purchase_ids } }, executor)
            .await
    }

    /// 读取申请的全部历史来源，供历史记录授权使用。
    ///
    /// # 参数
    /// `request_id` 为申请身份；`executor` 为原事务。
    /// # 返回
    /// 返回所有已冻结提交，禁止以当前申请代替旧版本。
    /// # 错误
    /// 数据库或反序列化失败时返回错误。
    pub async fn find_for_request(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchasePaymentRequestSubmissionSnapshot>> {
        self.repository.find_many(doc! { "request_id": request_id }, executor).await
    }

    /// 在申请提交与审批启动的同一事务中保存一次。
    ///
    /// # 参数
    /// * `snapshot` - 完整冻结的本轮提交。
    /// * `executor` - 本轮申请提交事务执行器。
    /// # 返回
    /// 成功插入时无返回值。
    /// # 错误
    /// 版本或实例重复、数据库写入失败时返回错误。
    pub async fn create(
        &self,
        snapshot: &PurchasePaymentRequestSubmissionSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.repository.create(snapshot, executor).await?;
        Ok(())
    }

    /// 按申请和精确提交版本读取完整原审核材料。
    ///
    /// # 参数
    /// * `request_id` - 申请身份。
    /// * `version` - 精确提交版本。
    /// * `executor` - 当前资格读取事务。
    /// # 返回
    /// 返回该版本快照，缺失时为空且不得补读当前申请。
    /// # 错误
    /// 查询或反序列化失败时返回错误。
    pub async fn find_for_version(
        &self,
        request_id: &str,
        version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchasePaymentRequestSubmissionSnapshot>> {
        self.repository
            .find_one(doc! {"request_id": request_id, "submission_version": i64::from(version)}, executor)
            .await
    }

    /// 通过精确审批实例读取该轮完整材料。
    ///
    /// # 参数
    /// * `instance_id` - 已经证明读取资格的审批实例身份。
    /// * `executor` - 当前资格读取事务。
    /// # 返回
    /// 返回该实例唯一提交快照，缺失时为空。
    /// # 错误
    /// 查询或反序列化失败时返回错误。
    pub async fn find_for_approval_instance(
        &self,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PurchasePaymentRequestSubmissionSnapshot>> {
        self.repository.find_one_by_field("approval_instance_id", instance_id, executor).await
    }
}
