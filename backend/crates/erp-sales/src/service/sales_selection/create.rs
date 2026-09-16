//! 创建选品册。

use erp_core::ids::{CustomerAccountId, SalesSelectionBookletId, SalesSelectionIdempotencyId, SkuId};
use persistence_core::{Executor, NoTransaction};

use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{
    CreateSalesSelectionBookletRequest, PrepareSalesSelectionRequest, SalesSelectionBookletView,
};
use crate::entity::sales_selection::{
    IdempotencyOperation, PoolSource, SalesSelectionBooklet, SalesSelectionBookletData,
    SalesSelectionIdempotency, SalesSelectionIdempotencyData, TierRule, normalize_idempotency_key,
    request_hash,
};
use crate::ports::sales_selection::SelectionCustomerPort;
use crate::repository::SalesSelectionExt;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 创建选品册并在同一事务内排队首次准备。
    ///
    /// 创建成功后册进入准备中；准备失败恢复仍回草稿。独立准备接口只用于重生成与重新准备。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor_id` - 创建人
    /// * `customer_port` - 客户事实
    /// * `executor` - 执行器；多集合写入须在事务中
    ///
    /// # 返回
    /// 返回已排队的准备中详情。
    ///
    /// # 错误
    /// 缺客户、形态、提交方式、幂等冲突、客户停用或无法排队准备时拒绝。
    pub async fn create(
        &self,
        req: CreateSalesSelectionBookletRequest,
        actor_id: &str,
        customer_port: &dyn SelectionCustomerPort,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let hash = request_hash(&serde_json::to_string(&req).unwrap_or_default());
        if let Some(replay) =
            self.replay_idempotency(IdempotencyOperation::Create, actor_id, &key, &hash, executor).await?
        {
            return Ok(replay);
        }
        let customer = customer_port.customer_fact(&req.customer_id).await?;
        if !customer.active {
            return Err(Error::ValidationError("客户已停用，不能创建选品册".into()));
        }
        let pool_source = PoolSource::new(
            req.pool_source_kind,
            req.pool_filter,
            req.sku_ids.map(|ids| ids.into_iter().map(SkuId::new).collect()),
        )?;
        let tiers = req
            .tiers
            .into_iter()
            .enumerate()
            .map(|(index, tier)| TierRule {
                tier_id: format!("tier-{index}"),
                name: tier.name,
                target_amount: tier.target_amount,
                tolerance: tier.tolerance,
                expected_count: tier.expected_count,
                sku_count: tier.sku_count,
            })
            .collect();
        let booklet_id = SalesSelectionBookletId::new(id_generator::next_id());
        let booklet = SalesSelectionBooklet::new(
            booklet_id.clone(),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new(customer.id),
                customer_no: customer.customer_no,
                customer_name: customer.display_name,
                sales_owner_user_id: req.sales_owner_user_id.clone(),
                business_org_unit_id: req.business_org_unit_id.clone(),
                form: req.form,
                submit_mode: req.submit_mode,
                pool_source,
                tiers,
                created_by: actor_id.to_string(),
            },
        )?;
        self.db.sales_selection_booklets().create(&booklet, executor).await?;
        let view = self
            .enqueue_prepare(
                PrepareSalesSelectionRequest::first_prepare(
                    booklet.base.id.clone(),
                    booklet.base.version,
                    key.clone(),
                ),
                actor_id,
                executor,
            )
            .await?;
        self.store_idempotency(
            IdempotencyStoreInput {
                operation: IdempotencyOperation::Create,
                scope_id: actor_id,
                key: &key,
                hash: &hash,
                result: &view,
                token_version: None,
                booklet_id: Some(booklet_id),
            },
            executor,
        )
        .await?;
        Ok(view)
    }

    /// 读取同键幂等结果。
    ///
    /// # 参数
    /// * `operation` - 操作
    /// * `scope_id` - 作用域
    /// * `key` - 幂等键
    /// * `hash` - 请求哈希
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 同键同请求返回原结果。
    ///
    /// # 错误
    /// 同键不同请求拒绝。
    pub(super) async fn replay_idempotency<T: serde::de::DeserializeOwned>(
        &self,
        operation: IdempotencyOperation,
        scope_id: &str,
        key: &str,
        hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<T>> {
        let found = self
            .db
            .sales_selection_idempotency()
            .find_by_key(operation.as_str(), scope_id, key, executor)
            .await?;
        let Some(record) = found else {
            return Ok(None);
        };
        record.ensure_same_request(hash).map_err(|error| Error::selection_conflict(error.to_string()))?;
        if operation.replays_live_booklet()
            && let Some(id) = &record.booklet_id
        {
            let book = self.load_booklet(id.as_ref(), executor).await?;
            let view = self.detail_view(&book, None, executor).await?;
            return serde_json::from_value(
                serde_json::to_value(view).map_err(|e| Error::Internal(e.to_string()))?,
            )
            .map(Some)
            .map_err(|e| Error::Internal(e.to_string()));
        }
        let value = serde_json::from_str(&record.result_json)
            .map_err(|error| Error::Internal(format!("幂等结果无法读取: {error}")))?;
        Ok(Some(value))
    }

    /// 写入幂等记录。
    ///
    /// # 参数
    /// * `input` - 幂等写入上下文
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 成功写入。
    ///
    /// # 错误
    /// 序列化或写入失败。
    pub(super) async fn store_idempotency<T: serde::Serialize>(
        &self,
        input: IdempotencyStoreInput<'_, T>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let record = Self::idempotency_record(input)?;
        self.db.sales_selection_idempotency().create(&record, executor).await?;
        Ok(())
    }

    /// 构造幂等记录（纯函数，事务内复用，不借用服务）。
    ///
    /// # 参数
    /// * `input` - 幂等写入上下文
    ///
    /// # 返回
    /// 返回待写入记录。
    ///
    /// # 错误
    /// 序列化失败。
    pub(super) fn idempotency_record<T: serde::Serialize>(
        input: IdempotencyStoreInput<'_, T>,
    ) -> Result<SalesSelectionIdempotency> {
        let result_json = serde_json::to_string(input.result)
            .map_err(|error| Error::Internal(format!("幂等结果无法保存: {error}")))?;
        Ok(SalesSelectionIdempotency::new(
            SalesSelectionIdempotencyId::new(id_generator::next_id()),
            SalesSelectionIdempotencyData {
                operation: input.operation,
                scope_id: input.scope_id.to_string(),
                key: input.key.to_string(),
                request_hash: input.hash.to_string(),
                result_json,
                token_version: input.token_version,
                booklet_id: input.booklet_id,
            },
        ))
    }

    /// 读取选品册，不存在则失败。
    ///
    /// # 参数
    /// * `id` - 册身份
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回选品册。
    ///
    /// # 错误
    /// 不存在。
    pub(super) async fn load_booklet(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<crate::entity::sales_selection::SalesSelectionBooklet> {
        self.db
            .sales_selection_booklets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("选品册不存在".into()))
    }
}

impl SalesSelectionService {
    /// 无事务读取选品册。
    ///
    /// # 参数
    /// * `id` - 身份
    ///
    /// # 返回
    /// 返回选品册。
    ///
    /// # 错误
    /// 不存在。
    pub async fn booklet(&self, id: &str) -> Result<crate::entity::sales_selection::SalesSelectionBooklet> {
        self.load_booklet(id, &mut NoTransaction).await
    }
}
