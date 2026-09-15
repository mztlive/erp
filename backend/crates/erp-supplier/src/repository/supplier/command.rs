use erp_core::ids::SupplierQualificationId;
use mongodb::bson::doc;
use persistence_core::{Executor, Result, mongo_ops};

use super::{
    SUPPLIER_ACCOUNTS, SUPPLIER_COMMERCIAL_PROFILE_REVISIONS, SUPPLIER_PROFILE_COMMANDS,
    SUPPLIER_QUALIFICATION_CAPABILITIES, SupplierRepository,
};
use crate::entity::supplier::{
    SupplierAccount, SupplierCommercialProfileRevision, SupplierProfileCommand,
    SupplierQualificationCapability,
};
use crate::repository::owned::SupplierProfileCommandRepository;

impl<'a> SupplierProfileCommandRepository<'a> {
    /// 按客户端幂等键读取已成功命令结果。
    ///
    /// # Errors
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileCommand>> {
        self.find_one(doc! { "idempotency_key": idempotency_key }, executor).await
    }
}

impl<'a> SupplierRepository<'a> {
    /// 按客户端幂等键读取供应商资料命令。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户端幂等键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已成功命令；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn profile_command(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProfileCommand>> {
        SupplierProfileCommandRepository::new(self.db, SUPPLIER_PROFILE_COMMANDS)
            .find_by_idempotency_key(idempotency_key, executor)
            .await
    }

    /// 创建供应商角色并写入首个商务结算版本（跨集合多步骤写入）。
    ///
    /// 依次写入 `supplier_commercial_profile_revisions` 与 `supplier_accounts`，
    /// 保证「商务版本 + 供应商角色」原子可见（数据模型 §6.2：供应商角色
    /// 携带 `current_commercial_profile_revision_id` 指向当前版本）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有版本没有供应商角色的
    /// 半成品；Service 必须通过 `persistence_core::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `supplier` - 待写入的供应商角色（`current_commercial_profile_revision_id`
    ///   必须已指向 `revision`）
    /// * `revision` - 待写入的首个商务结算版本
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当供应商编号或主体归属违反唯一索引（透出 [`persistence_core::Error::DuplicateKey`]）
    /// 或 MongoDB 写入失败时返回错误。
    pub async fn create_supplier_with_initial_profile(
        &self,
        supplier: &SupplierAccount,
        revision: &SupplierCommercialProfileRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierCommercialProfileRevision>(SUPPLIER_COMMERCIAL_PROFILE_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_one(&self.db.collection::<SupplierAccount>(SUPPLIER_ACCOUNTS), supplier, executor)
            .await
    }

    /// 在同一事务内整体替换一份资质的适用能力集合。
    ///
    /// 调用方必须先校验能力均属于同一供应商，并传入事务执行器。
    ///
    /// # Errors
    /// 删除旧关联或写入新关联失败时返回错误。
    pub async fn replace_qualification_capabilities(
        &self,
        qualification_id: &SupplierQualificationId,
        links: Vec<SupplierQualificationCapability>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::delete_many(
            &self.db.collection::<SupplierQualificationCapability>(SUPPLIER_QUALIFICATION_CAPABILITIES),
            doc! { "qualification_id": qualification_id.to_string() },
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<SupplierQualificationCapability>(SUPPLIER_QUALIFICATION_CAPABILITIES),
            links,
            executor,
        )
        .await
    }
}
