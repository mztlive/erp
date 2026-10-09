//! 创建选品册。

use erp_core::ids::{CustomerAccountId, SalesSelectionBookletId, SalesSelectionIdempotencyId, SkuId};
use persistence_core::{Executor, NoTransaction};

use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{
    CreateSalesSelectionBookletRequest, PrepareSalesSelectionRequest, SalesSelectionBookletView,
};
use crate::entity::sales_selection::{
    IdempotencyOperation, PoolSource, SalesSelectionBooklet, SalesSelectionBookletData,
    SalesSelectionIdempotency, SalesSelectionIdempotencyData, SelectionRequestFingerprint, TierRule,
    normalize_idempotency_key,
};
use crate::ports::sales_selection::{SelectionCustomerFact, SelectionCustomerPort};
use crate::repository::SalesSelectionExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 创建选品册并在同一事务内排队首次准备。
    ///
    /// 创建成功后册进入准备中；准备失败恢复仍回草稿。独立准备接口只用于重生成与重新准备。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor_id` - 创建人
    /// * `password_hash` - 事务外生成的慢哈希
    /// * `fingerprint` - 组合层对完整原请求生成的带密钥指纹
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
        password_hash: String,
        fingerprint: SelectionRequestFingerprint,
        customer_port: &dyn SelectionCustomerPort,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let hash = fingerprint.as_str();
        if let Some(replay) =
            self.replay_idempotency(IdempotencyOperation::Create, actor_id, &key, hash, executor).await?
        {
            return Ok(replay);
        }
        let customer = customer_port.customer_fact(&req.customer_id).await?;
        if !customer.active {
            return Err(Error::ValidationError("客户已停用，不能创建选品册".into()));
        }
        let booklet_id = SalesSelectionBookletId::new(id_generator::next_id());
        let booklet = create_booklet(booklet_id.clone(), req, customer, actor_id, password_hash)?;
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
                hash,
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
    /// 同键同请求返回 `Ok(Some(原结果))`；无同键记录时返回 `Ok(None)`。
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

fn create_booklet(
    id: SalesSelectionBookletId,
    req: CreateSalesSelectionBookletRequest,
    customer: SelectionCustomerFact,
    actor_id: &str,
    password_hash: String,
) -> Result<SalesSelectionBooklet> {
    let pool_source = PoolSource::new(
        req.pool_source_kind,
        req.pool_filter,
        req.sku_ids.map(|ids| ids.into_iter().map(SkuId::new).collect()),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
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
    SalesSelectionBooklet::new(
        id,
        SalesSelectionBookletData {
            customer_id: CustomerAccountId::new(customer.id),
            customer_no: customer.customer_no,
            customer_name: customer.display_name,
            sales_owner_user_id: req.sales_owner_user_id,
            business_org_unit_id: req.business_org_unit_id,
            form: req.form,
            submit_mode: req.submit_mode,
            access_password_hash: Some(password_hash),
            per_person_budget: req.per_person_budget,
            voucher_count: req.voucher_count,
            pool_source,
            tiers,
            created_by: actor_id.to_string(),
        },
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde_json::json;

    use super::*;
    use crate::dto::sales_selection::SalesSelectionPasswordRequest;
    use crate::entity::sales_selection::{LinkTokenCrypto, SelectionForm, SubmitMode, request_hash};

    fn assert_sensitive_record<T: Serialize>(
        operation: IdempotencyOperation,
        original: &T,
        changed_password: &T,
        password: &str,
    ) {
        let crypto = LinkTokenCrypto::from_secret(b"application-secret-outside-database");
        let first = crypto.request_fingerprint(operation, original).unwrap();
        let same = crypto.request_fingerprint(operation, original).unwrap();
        let changed = crypto.request_fingerprint(operation, changed_password).unwrap();
        let record = SalesSelectionService::idempotency_record(IdempotencyStoreInput {
            operation,
            scope_id: "sales-actor",
            key: "same-key",
            hash: first.as_str(),
            result: &json!({"id": "book", "access_password_set": true}),
            token_version: None,
            booklet_id: Some(SalesSelectionBookletId::new("book")),
        })
        .unwrap();
        assert_eq!(record.idempotency_key, "same-key");
        assert!(record.ensure_same_request(same.as_str()).is_ok());
        assert!(record.ensure_same_request(changed.as_str()).is_err());
        let stored = serde_json::to_string(&record).unwrap();
        let unkeyed_request = request_hash(&serde_json::to_string(original).unwrap());
        assert!(!stored.contains(password));
        assert!(!stored.contains(&request_hash(password)));
        assert!(!stored.contains(&unkeyed_request));
        assert_eq!(record.request_hash, first.as_str());
    }

    #[test]
    fn create_password_uses_keyed_idempotency_record() {
        let req = CreateSalesSelectionBookletRequest {
            idempotency_key: "same-key".into(),
            customer_id: "customer".into(),
            sales_owner_user_id: "sales".into(),
            business_org_unit_id: "department".into(),
            form: SelectionForm::SingleSku,
            submit_mode: SubmitMode::ByQuantity,
            access_password: "original-create-password".into(),
            per_person_budget: None,
            voucher_count: None,
            pool_source_kind: crate::entity::sales_selection::PoolSourceKind::Filter,
            pool_filter: None,
            sku_ids: None,
            tiers: Vec::new(),
        };
        let changed = CreateSalesSelectionBookletRequest {
            access_password: "changed-create-password".into(),
            ..req.clone()
        };
        assert_sensitive_record(IdempotencyOperation::Create, &req, &changed, &req.access_password);
    }

    #[test]
    fn password_maintenance_keeps_full_request_identity_without_fast_password_verifier() {
        let req = SalesSelectionPasswordRequest {
            expected_version: 3,
            idempotency_key: "same-key".into(),
            access_password: "original-maintenance-password".into(),
        };
        let changed = SalesSelectionPasswordRequest {
            access_password: "changed-maintenance-password".into(),
            ..req.clone()
        };
        assert_sensitive_record(
            IdempotencyOperation::SetAccessPassword,
            &("book", &req),
            &("book", &changed),
            &req.access_password,
        );
        let crypto = LinkTokenCrypto::from_secret(b"application-secret-outside-database");
        assert_ne!(
            crypto
                .request_fingerprint(IdempotencyOperation::SetAccessPassword, &("book", &req))
                .unwrap()
                .as_str(),
            crypto
                .request_fingerprint(IdempotencyOperation::SetAccessPassword, &("other-book", &req))
                .unwrap()
                .as_str()
        );
    }
}
