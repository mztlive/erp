//! 在调用方事务内复制配置、冻结商品池与当前有效陈列。

use erp_core::ids::{
    CustomerAccountId, SalesSelectionBookletId, SalesSelectionDisplayItemId, SalesSelectionPoolMemberId,
};
use persistence_core::Executor;

use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{CopySalesSelectionBookletRequest, SalesSelectionBookletView};
use crate::entity::sales_selection::{
    IdempotencyOperation, SalesSelectionBooklet, SalesSelectionPoolMember, SelectionRequestFingerprint,
    normalize_idempotency_key,
};
use crate::ports::sales_selection::SelectionCustomerFact;
use crate::repository::SalesSelectionExt;
use crate::repository::sales_selection::SalesSelectionDomainRepository;
use crate::{Error, Result};

impl SalesSelectionService {
    /// 复制已授权原册，所有读写使用调用方同一事务。
    /// 调用方须在同一执行器核验原册、新客户及创建候选；actor_id 只用于领域留痕。
    /// 返回新册详情；同键同请求重放返回同一本新册。
    /// 客户停用、原册版本变化、准备中、幂等冲突或持久化失败时拒绝。
    ///
    /// 输入约定：
    /// - `source`：事务内完成对象授权的原册
    /// - `req`：原册版本、新客户与幂等键
    /// - `customer`：已授权新客户事实
    /// - `actor_id`：复制操作人
    /// - `fingerprint`：绑定原册身份和完整请求的指纹
    /// - `executor`：调用方事务执行器
    pub async fn copy(
        &self,
        source: &SalesSelectionBooklet,
        req: &CopySalesSelectionBookletRequest,
        customer: SelectionCustomerFact,
        actor_id: &str,
        fingerprint: &SelectionRequestFingerprint,
        executor: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let hash = fingerprint.as_str();
        if let Some(replay) =
            self.replay_idempotency(IdempotencyOperation::Copy, actor_id, &key, hash, executor).await?
        {
            return Ok(replay);
        }

        let id = SalesSelectionBookletId::new(id_generator::next_id());
        let book = copy_booklet(source, req.expected_version, id.clone(), customer, actor_id)?;
        self.db.sales_selection_booklets().create(&book, executor).await?;
        self.copy_batch(source, &book, executor).await?;

        let view = self.detail_view(&book, None, executor).await?;
        self.store_idempotency(
            IdempotencyStoreInput {
                operation: IdempotencyOperation::Copy,
                scope_id: actor_id,
                key: &key,
                hash,
                result: &view,
                token_version: None,
                booklet_id: Some(id),
            },
            executor,
        )
        .await?;
        Ok(view)
    }

    async fn copy_batch(
        &self,
        source: &SalesSelectionBooklet,
        target: &SalesSelectionBooklet,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (Some(source_batch), Some(target_batch)) = (&source.current_batch_id, &target.current_batch_id)
        else {
            return Ok(());
        };
        let domain = SalesSelectionDomainRepository::new(&self.db);
        let target_id = SalesSelectionBookletId::new(target.base.id.clone());
        let members = domain
            .list_pool_members(&source.base.id, source_batch, executor)
            .await?
            .into_iter()
            .map(|member| {
                SalesSelectionPoolMember::new(
                    SalesSelectionPoolMemberId::new(id_generator::next_id()),
                    target_id.clone(),
                    target_batch.clone(),
                    member.sku,
                )
            })
            .collect::<Vec<_>>();

        let items = domain
            .list_effective_items(&source.base.id, source_batch, executor)
            .await?
            .into_iter()
            .map(|item| {
                item.copied(
                    SalesSelectionDisplayItemId::new(id_generator::next_id()),
                    target_id.clone(),
                    target_batch.clone(),
                )
            })
            .collect::<Vec<_>>();

        domain.insert_members_and_items(&members, &items, executor).await?;
        Ok(())
    }
}

fn copy_booklet(
    source: &SalesSelectionBooklet,
    expected_version: u64,
    id: SalesSelectionBookletId,
    customer: SelectionCustomerFact,
    actor_id: &str,
) -> Result<SalesSelectionBooklet> {
    source.ensure_version(expected_version).map_err(|error| Error::selection_conflict(error.to_string()))?;
    if !customer.active {
        return Err(Error::ValidationError("客户已停用，不能复制选品册".into()));
    }
    source
        .copied(
            id,
            CustomerAccountId::new(customer.id),
            customer.customer_no,
            customer.display_name,
            actor_id,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::sales_selection::{
        LinkTokenCrypto, PoolFilterSnapshot, PoolSource, PoolSourceKind, SalesSelectionBookletData,
        SelectionForm, SubmitMode,
    };

    fn source() -> SalesSelectionBooklet {
        SalesSelectionBooklet::new(
            SalesSelectionBookletId::new("source"),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new("old-customer"),
                customer_no: "C001".into(),
                customer_name: "原客户".into(),
                sales_owner_user_id: "owner".into(),
                business_org_unit_id: "org".into(),
                form: SelectionForm::SingleSku,
                submit_mode: SubmitMode::ByQuantity,
                access_password_hash: Some("password-hash".into()),
                pool_source: PoolSource::new(
                    PoolSourceKind::Filter,
                    Some(PoolFilterSnapshot::default()),
                    None,
                )
                .unwrap(),
                tiers: vec![],
                created_by: "creator".into(),
            },
        )
        .unwrap()
    }

    fn customer(active: bool) -> SelectionCustomerFact {
        SelectionCustomerFact {
            id: "new-customer".into(),
            customer_no: "C002".into(),
            display_name: "新客户".into(),
            active,
        }
    }

    #[test]
    fn copy_command_rejects_stale_version_and_inactive_customer_before_writes() {
        let original = source();
        let id = SalesSelectionBookletId::new("target");
        let copied =
            copy_booklet(&original, original.base.version, id.clone(), customer(true), "operator").unwrap();
        assert_eq!(copied.customer_id.as_ref(), "new-customer");
        let stale =
            copy_booklet(&original, original.base.version + 1, id.clone(), customer(true), "operator")
                .unwrap_err();
        assert!(matches!(stale, Error::SelectionConflict(_)));
        let inactive =
            copy_booklet(&original, original.base.version, id, customer(false), "operator").unwrap_err();
        assert!(inactive.to_string().contains("客户已停用"));
    }

    #[test]
    fn copy_retry_record_binds_source_customer_and_version() {
        let crypto = LinkTokenCrypto::from_secret(b"copy-test-secret");
        let req = CopySalesSelectionBookletRequest {
            expected_version: 3,
            customer_id: "new-customer".into(),
            idempotency_key: "copy-1".into(),
        };
        let fingerprint = crypto.request_fingerprint(IdempotencyOperation::Copy, &("source", &req)).unwrap();
        let record = SalesSelectionService::idempotency_record(IdempotencyStoreInput {
            operation: IdempotencyOperation::Copy,
            scope_id: "operator",
            key: &req.idempotency_key,
            hash: fingerprint.as_str(),
            result: &serde_json::json!({"id": "target"}),
            token_version: None,
            booklet_id: Some(SalesSelectionBookletId::new("target")),
        })
        .unwrap();
        let replay = crypto.request_fingerprint(IdempotencyOperation::Copy, &("source", &req)).unwrap();
        assert!(record.ensure_same_request(replay.as_str()).is_ok());
        assert_eq!(record.booklet_id.unwrap().as_ref(), "target");
        for (source, changed) in [
            ("other-source", req.clone()),
            (
                "source",
                CopySalesSelectionBookletRequest { customer_id: "other-customer".into(), ..req.clone() },
            ),
            ("source", CopySalesSelectionBookletRequest { expected_version: 4, ..req.clone() }),
        ] {
            let changed = crypto.request_fingerprint(IdempotencyOperation::Copy, &(source, changed)).unwrap();
            assert_ne!(fingerprint.as_str(), changed.as_str());
        }
    }
}
