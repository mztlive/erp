use application_core::CommandReceipt;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc, serialize_to_bson};
use mongodb::options::{FindOptions, IndexOptions};
use mongodb::{Database, IndexModel};
use persistence_core::{Executor, Repository, mongo_ops};

use super::{ConfirmedCooperation, CooperationApplication, CooperationReceipt, CooperationStatus};
use crate::{Error, Result, SupplierAccount, SupplierExt};

/// 合作条款申请集合。
pub const COOPERATION_APPLICATIONS: &str = "supplier_cooperation_applications";
/// 合作条款命令回执集合。
pub const COOPERATION_RECEIPTS: &str = "supplier_cooperation_command_receipts";

/// 合作条款仓储；事务边界始终由调用方控制。
pub struct CooperationRepository<'a> {
    db: &'a Database,
}

impl<'a> CooperationRepository<'a> {
    /// 创建本域仓储。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库。
    /// # 返回
    /// 返回仓储。
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 读取内部审核指定的申请；调用方必须执行 DataScope 与当前任务资格验证。
    ///
    /// # 参数
    /// * `id` - 内部任务指向的精确申请标识。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 返回未删除申请或空结果。
    /// # 错误
    /// 数据库读取失败时拒绝。
    pub async fn find_any(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CooperationApplication>> {
        Ok(Repository::new(self.db, COOPERATION_APPLICATIONS).find_by_id(id, executor).await?)
    }

    /// 统计当前供应商的同一筛选范围。
    ///
    /// # 参数
    /// * `supplier_id` - 服务器授权供应商。
    /// * `status` - 同列表的可选状态。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 返回未删除申请总数。
    /// # 错误
    /// 范围或数据库读取失败时拒绝。
    pub async fn count(
        &self,
        supplier_id: &str,
        status: Option<CooperationStatus>,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        let filter = list_filter(supplier_id, status)?;
        Ok(mongo_ops::count_documents(
            &self.db.collection::<CooperationApplication>(COOPERATION_APPLICATIONS),
            filter,
            executor,
        )
        .await?)
    }

    /// 按工作台已收窄的精确申请 ID 获取正式任务事实。
    ///
    /// # 参数
    /// * `ids` - 已由内部工作项限定的申请标识。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 返回未删除申请；对象责任与任务资格仍由内部调用方校验。
    /// # 错误
    /// 数据库读取失败时拒绝。
    pub async fn list_active_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CooperationApplication>> {
        Ok(Repository::new(self.db, COOPERATION_APPLICATIONS).list_active_by_ids(ids, executor).await?)
    }

    /// 按当前授权供应商读取申请，范围外与不存在统一为空。
    ///
    /// # 参数
    /// * `id` - 申请标识。
    /// * `supplier_id` - 服务器授权供应商。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 返回可见申请或空结果。
    /// # 错误
    /// 数据库错误时拒绝。
    pub async fn get(
        &self,
        id: &str,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CooperationApplication>> {
        let mut filter = scoped_filter(supplier_id);
        filter.insert("id", id);
        Ok(mongo_ops::find_one(&self.db.collection(COOPERATION_APPLICATIONS), filter, executor).await?)
    }

    /// 读取供应商申请列表，采用稳定排序和有界分页。
    ///
    /// # 参数
    /// * `supplier_id` - 已鉴权范围。
    /// * `status` - 可选申请状态。
    /// * `offset` - 列表偏移。
    /// * `limit` - 页大小，范围 1 至 100。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 返回可见申请集合。
    /// # 错误
    /// 分页非法或数据库读取失败时拒绝。
    pub async fn list(
        &self,
        supplier_id: &str,
        status: Option<CooperationStatus>,
        offset: u64,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<CooperationApplication>> {
        if limit == 0 || limit > 100 || supplier_id.trim().is_empty() {
            return Err(Error::ValidationError("申请分页或供应商范围无效".into()));
        }
        let filter = list_filter(supplier_id, status)?;
        let options = FindOptions::builder()
            .skip(offset)
            .limit(i64::from(limit))
            .sort(doc! { "created_at": -1, "id": -1 })
            .build();
        Ok(mongo_ops::find_many(&self.db.collection(COOPERATION_APPLICATIONS), filter, options, executor)
            .await?)
    }

    /// 插入尚未提交的供应商草稿。
    ///
    /// # 参数
    /// * `application` - 通过实体构造的草稿。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 返回空结果。
    /// # 错误
    /// 草稿状态非法、重复身份或数据库错误时拒绝。
    pub async fn create(
        &self,
        application: &CooperationApplication,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if application.status != CooperationStatus::Draft
            || !application.submissions.is_empty()
            || !application.decisions.is_empty()
            || application.result.is_some()
        {
            return Err(Error::ValidationError("新申请必须从草稿开始".into()));
        }
        Ok(Repository::new(self.db, COOPERATION_APPLICATIONS).create(application, executor).await?)
    }

    /// 保存申请并以持久化版本进行 CAS，保护既有冻结历史。
    ///
    /// # 参数
    /// * `application` - 实体动作产生的申请。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 成功后原地更新持久化版本。
    /// # 错误
    /// 历史被改写、状态不存在或并发更新时拒绝。
    pub async fn update(
        &self,
        application: &mut CooperationApplication,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let before = self
            .get(&application.base.id, &application.supplier_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("合作条款申请不可见".into()))?;
        application.ensure_history_preserved(&before)?;
        Ok(Repository::new(self.db, COOPERATION_APPLICATIONS).update(application, executor).await?)
    }

    /// 读取原命令实际结果并校验身份及载荷。
    ///
    /// # 参数
    /// * `command` - 当前规范命令。
    /// * `supplier_id` - 当前授权范围。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 返回可重放回执或空结果。
    /// # 错误
    /// 同幂等键异载荷或数据库读取失败时拒绝。
    pub async fn receipt(
        &self,
        command: &CommandReceipt,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CooperationReceipt>> {
        let result: Option<CooperationReceipt> = mongo_ops::find_one(
            &self.db.collection(COOPERATION_RECEIPTS),
            doc! { "id": command.id(), "supplier_id": supplier_id },
            executor,
        )
        .await?;
        if let Some(receipt) = &result {
            receipt.ensure_replayable(command, supplier_id)?;
        }
        Ok(result)
    }

    /// 同业务状态、任务及审计事务写入成功回执。
    ///
    /// # 参数
    /// * `receipt` - 原命令结果。
    /// * `executor` - 调用方事务执行器。
    /// # 返回
    /// 返回空结果。
    /// # 错误
    /// 重复命令或数据库失败时拒绝。
    pub async fn record_receipt(
        &self,
        receipt: &CooperationReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(Repository::new(self.db, COOPERATION_RECEIPTS).create(receipt, executor).await?)
    }

    /// 原子切换正式商务指针并插入商务修订；不自行开启事务。
    ///
    /// # 参数
    /// * `before` - 同执行器读取的旧供应商。
    /// * `plan` - 纯规则生成的确认计划。
    /// * `executor` - 必須由 Process 传入当前事务执行器。
    /// # 返回
    /// 成功后 plan supplier 更新为实际写入版本。
    /// # 错误
    /// 当前 supplier 版本、状态或 profile 指针变更时返回冲突。
    pub async fn apply_confirmed(
        &self,
        before: &SupplierAccount,
        plan: &mut ConfirmedCooperation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if executor.session().is_none() {
            return Err(Error::Internal("合作条款确认必须使用调用方事务执行器".into()));
        }
        ensure_confirmed_plan(before, plan)?;
        let version =
            i64::try_from(before.base.version).map_err(|_| Error::ConflictError("供应商版本超限".into()))?;
        let next = i64::try_from(plan.result.supplier_version)
            .map_err(|_| Error::ConflictError("供应商版本超限".into()))?;
        let pointer = before.current_commercial_profile_revision_id.as_ref().map(ToString::to_string);
        let filter = doc! { "id": &before.base.id, "version": version, "status": "active",
        "current_commercial_profile_revision_id": pointer, "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        let written = mongo_ops::update_one(
            &self.db.collection::<SupplierAccount>(Database::SUPPLIER_ACCOUNTS),
            filter,
            doc! { "$set": { "current_commercial_profile_revision_id": &plan.profile.base.id,
            "updated_by": &plan.result.confirmed_by,
            "version": next, "updated_at": i64::try_from(plan.result.confirmed_at)
                .map_err(|_| Error::ValidationError("确认时间超限".into()))? } },
            false,
            executor,
        )
        .await?;
        if written.matched_count != 1 {
            return Err(Error::ConflictError("供应商商务档案已改变，请重新提交".into()));
        }
        self.db.supplier_commercial_profile_revisions().create(&plan.profile, executor).await?;
        plan.supplier.base.version = plan.result.supplier_version;
        plan.supplier.base.updated_at = plan.result.confirmed_at;
        Ok(())
    }
}

fn ensure_confirmed_plan(before: &SupplierAccount, plan: &ConfirmedCooperation) -> Result<()> {
    if plan.supplier.base.id != before.base.id
        || plan.profile.supplier_id.to_string() != before.base.id
        || plan.profile.base.id != plan.result.profile_id
        || plan.profile.revision.revision_no != plan.result.profile_revision_no
        || before.base.version.checked_add(1) != Some(plan.result.supplier_version)
        || plan.supplier.current_commercial_profile_revision_id.as_ref().map(ToString::to_string).as_deref()
            != Some(plan.profile.base.id.as_str())
    {
        return Err(Error::ValidationError("合作条款正式写入计划不一致".into()));
    }
    Ok(())
}

fn scoped_filter(supplier_id: &str) -> Document {
    doc! { "supplier_id": supplier_id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON }
}

fn list_filter(supplier_id: &str, status: Option<CooperationStatus>) -> Result<Document> {
    if supplier_id.trim().is_empty() {
        return Err(Error::Forbidden("供应商绑定无效".into()));
    }
    let mut filter = scoped_filter(supplier_id);
    if let Some(status) = status {
        filter.insert(
            "status",
            serialize_to_bson(&status).map_err(|error| Error::Internal(error.to_string()))?,
        );
    }
    Ok(filter)
}

/// 注册门户申请及回执唯一身份、范围与稳定分页索引。
///
/// # 参数
/// * `db` - 当前数据库。
/// # 返回
/// 返回空结果。
/// # 错误
/// 唯一性冲突或建索引失败时拒绝启动。
pub async fn ensure_indexes(db: &Database) -> persistence_core::Result<()> {
    for (collection, indexes) in [
        (
            COOPERATION_APPLICATIONS,
            vec![
                index("uk_supplier_cooperation_application_id", doc! { "id": 1 }, true),
                index(
                    "idx_supplier_cooperation_application_supplier",
                    doc! { "supplier_id": 1,
                    "deleted_at": 1, "created_at": -1, "id": -1 },
                    false,
                ),
                index(
                    "idx_supplier_cooperation_application_supplier_status",
                    doc! { "supplier_id": 1,
                    "status": 1, "deleted_at": 1, "created_at": -1, "id": -1 },
                    false,
                ),
            ],
        ),
        (COOPERATION_RECEIPTS, vec![index("uk_supplier_cooperation_receipt_id", doc! { "id": 1 }, true)]),
    ] {
        db.collection::<Document>(collection).create_indexes(indexes).await?;
    }
    Ok(())
}

fn index(name: &str, keys: Document, unique: bool) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.to_string()).unique(unique).build())
        .build()
}
