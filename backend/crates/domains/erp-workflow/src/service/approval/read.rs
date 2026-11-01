//! 审批定义和启动收据读取；业务入口保留授权、绑定版本策略和恢复身份核验。

use async_trait::async_trait;
use bpm::engine::DefinitionGraph;
use bpm::ids::ApprovalProcessDefinitionId;
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{ApprovalCommandReceipt, IdempotencyKey, SubjectRef};
use mongodb::Database;
use persistence_core::Executor;

use super::execution::idempotency::{normalize_idempotency_key, start_scope_candidates};
use super::process_kind::process_kind_of;
use crate::entity::document_registry::DocumentType;
use crate::entity::document_registry::business_document::ApprovalDefinitionBinding;
use crate::{BpmExt, Error, Result};

/// 按冻结绑定的定义主键读取定义图。
///
/// # 参数
/// * `db` - 审批数据库。
/// * `binding` - 单据冻结的定义绑定。
/// * `missing_message` - 原业务入口的定义缺失文案。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回绑定主键对应的定义图；本入口不增加业务版本校验。
///
/// # 错误
/// 定义缺失时返回指定冲突文案；仓储失败保留工作流错误分类。
pub async fn bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    missing_message: &str,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    definition_from_store(db, binding, None, missing_message, executor).await
}

/// 按冻结绑定的定义主键及业务版本读取定义图。
///
/// # 参数
/// * `db` - 审批数据库。
/// * `binding` - 单据冻结的定义绑定。
/// * `missing_message` - 定义缺失或版本不匹配时沿用的冲突文案。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回主键及业务版本均匹配的定义图。
///
/// # 错误
/// 定义缺失或版本不匹配时返回指定冲突文案；仓储失败保留工作流错误分类。
pub async fn exact_bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    missing_message: &str,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    definition_from_store(db, binding, Some(binding.approval_definition_version), missing_message, executor)
        .await
}

/// 按单据启动身份的当前及历史候选作用域查找首个收据。
///
/// # 参数
/// * `db` - 审批数据库。
/// * `document_type` - 入口确定的单据类型。
/// * `subject` - 冻结业务对象引用。
/// * `subject_version` - 冻结业务对象版本。
/// * `idempotency_key` - 尚未规范化的幂等键。
/// * `executor` - 所有查询共用的调用方执行器。
///
/// # 返回
/// 返回当前作用域优先、历史作用域其次的首个命中收据；均未命中返回空值。
/// 收据存在不代表载荷及结果已核验，调用方继续执行精确身份和结果核验。
///
/// # 错误
/// 幂等键或作用域非法、任一仓储读取失败时立即返回错误。
pub async fn start_receipt(
    db: &Database,
    document_type: DocumentType,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    start_receipt_from_store(db, document_type, subject, subject_version, idempotency_key, executor).await
}

async fn start_receipt_from_store(
    store: &impl ApprovalReadStore,
    document_type: DocumentType,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(document_type);
    let scopes = start_scope_candidates(
        process_kind.as_str(),
        subject.subject_kind(),
        subject.subject_id(),
        subject_version,
    )?;
    receipt_from_store(store, &scopes, &key, executor).await
}

/// 按调用方提供的精确候选顺序查找首个启动收据。
///
/// # 参数
/// * `db` - 审批数据库。
/// * `scopes` - 已由命令身份合同生成的有序候选作用域。
/// * `key` - 已规范化的幂等键。
/// * `executor` - 所有查询共用的调用方执行器。
///
/// # 返回
/// 返回首个命中收据；空候选或全部未命中返回空值。不核验收据载荷。
///
/// # 错误
/// 任一仓储读取失败时立即返回，不继续读取后续候选。
pub async fn start_receipt_for_scopes(
    db: &Database,
    scopes: &[String],
    key: &IdempotencyKey,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    receipt_from_store(db, scopes, key, executor).await
}

#[async_trait]
trait ApprovalReadStore: Sync {
    async fn definition(
        &self,
        id: &ApprovalProcessDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<DefinitionGraph>>;
    async fn receipt(
        &self,
        scope: &str,
        key: &IdempotencyKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalCommandReceipt>>;
}

#[async_trait]
impl ApprovalReadStore for Database {
    async fn definition(
        &self,
        id: &ApprovalProcessDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<DefinitionGraph>> {
        Ok(self.bpm_workflow().load_definition_graph(id, executor).await?)
    }

    async fn receipt(
        &self,
        scope: &str,
        key: &IdempotencyKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalCommandReceipt>> {
        Ok(self
            .bpm_workflow()
            .find_command_receipt(ApprovalCommandKind::StartApproval, scope, key, executor)
            .await?)
    }
}

async fn definition_from_store(
    store: &impl ApprovalReadStore,
    binding: &ApprovalDefinitionBinding,
    expected_version: Option<u32>,
    missing_message: &str,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    store
        .definition(&binding.approval_process_definition_id, executor)
        .await?
        .filter(|graph| expected_version.is_none_or(|version| graph.definition.definition_version == version))
        .ok_or_else(|| Error::ConflictError(missing_message.to_string()))
}

async fn receipt_from_store(
    store: &impl ApprovalReadStore,
    scopes: &[String],
    key: &IdempotencyKey,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    for scope in scopes {
        let receipt = store.receipt(scope, key, executor).await?;
        if receipt.is_some() {
            return Ok(receipt);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use bpm::ids::ApprovalCommandReceiptId;
    use bpm::model::{ApprovalProcessDefinition, ParticipantId, Timestamp};
    use erp_core::common::time::Instant;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::service::approval::execution::idempotency::{StartIdentityParams, start_identity};

    type ReadCall = (String, String, usize);

    struct FakeStore {
        graph: Mutex<Option<Result<Option<DefinitionGraph>>>>,
        receipts: Mutex<VecDeque<Result<Option<ApprovalCommandReceipt>>>>,
        calls: Mutex<Vec<ReadCall>>,
    }

    impl FakeStore {
        fn new(receipts: Vec<Result<Option<ApprovalCommandReceipt>>>) -> Self {
            Self {
                graph: Mutex::new(None),
                receipts: Mutex::new(receipts.into()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn record(&self, scope: &str, key: &str, executor: &mut dyn Executor) {
            self.calls.lock().unwrap().push((
                scope.to_string(),
                key.to_string(),
                executor as *mut dyn Executor as *mut () as usize,
            ));
        }
    }

    #[async_trait]
    impl ApprovalReadStore for FakeStore {
        async fn definition(
            &self,
            id: &ApprovalProcessDefinitionId,
            executor: &mut dyn Executor,
        ) -> Result<Option<DefinitionGraph>> {
            self.record(id.as_ref(), "definition", executor);
            self.graph.lock().unwrap().take().unwrap()
        }

        async fn receipt(
            &self,
            scope: &str,
            key: &IdempotencyKey,
            executor: &mut dyn Executor,
        ) -> Result<Option<ApprovalCommandReceipt>> {
            self.record(scope, key.as_str(), executor);
            self.receipts.lock().unwrap().pop_front().unwrap()
        }
    }

    fn binding() -> ApprovalDefinitionBinding {
        ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("definition-1"),
            2,
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }

    fn graph(version: u32) -> DefinitionGraph {
        DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                ApprovalProcessDefinitionId::new("definition-1"),
                process_kind_of(DocumentType::StockAdjustment),
                version,
                "库存调整审批",
                "entry",
                ParticipantId::new("actor-1").unwrap(),
                Timestamp::from_unix_secs(1).unwrap(),
            )
            .unwrap(),
            nodes: Vec::new(),
            transitions: Vec::new(),
        }
    }

    fn receipt() -> ApprovalCommandReceipt {
        let identity = start_identity(StartIdentityParams {
            idempotency_key: normalize_idempotency_key("key-1").unwrap(),
            process_kind: process_kind_of(DocumentType::StockAdjustment).as_str(),
            subject_kind: DocumentType::StockAdjustment.as_str(),
            subject_id: "adjustment-1",
            subject_version: 1,
            binding_id: "definition-1",
            definition_version: 2,
            actor_participant_id: "actor-1",
        })
        .unwrap();
        ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("receipt-1"),
            identity.current(),
            "instance-1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn standard_start_lookup_normalizes_key_and_queries_current_before_legacy() {
        let store = FakeStore::new(vec![Ok(None), Ok(Some(receipt()))]);
        let subject = SubjectRef::new("stock_adjustment", "adjustment-1").unwrap();
        let result = start_receipt_from_store(
            &store,
            DocumentType::StockAdjustment,
            &subject,
            1,
            "  key-1  ",
            &mut NoTransaction,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.base.id, "receipt-1");
        let expected_scopes = start_scope_candidates(
            process_kind_of(DocumentType::StockAdjustment).as_str(),
            subject.subject_kind(),
            subject.subject_id(),
            1,
        )
        .unwrap();
        let calls = store.calls.lock().unwrap();
        assert_eq!(calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(), expected_scopes);
        assert!(calls.iter().all(|call| call.1 == "key-1"));
    }

    #[tokio::test]
    async fn invalid_start_key_is_rejected_before_any_read() {
        let store = FakeStore::new(vec![]);
        let subject = SubjectRef::new("stock_adjustment", "adjustment-1").unwrap();
        let error = start_receipt_from_store(
            &store,
            DocumentType::StockAdjustment,
            &subject,
            1,
            "   ",
            &mut NoTransaction,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ValidationError(_)));
        assert!(store.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn receipt_candidates_keep_order_key_and_executor_and_stop_after_first_match() {
        let store = FakeStore::new(vec![Ok(None), Ok(Some(receipt())), Ok(Some(receipt()))]);
        let scopes = vec!["current".to_string(), "legacy".to_string(), "unused".to_string()];
        let key = normalize_idempotency_key("  key-1  ").unwrap();
        let mut executor = NoTransaction;
        let pointer = &mut executor as *mut NoTransaction as usize;
        let result = receipt_from_store(&store, &scopes, &key, &mut executor).await.unwrap().unwrap();
        assert_eq!(result.base.id, "receipt-1");
        assert_eq!(
            *store.calls.lock().unwrap(),
            vec![
                ("current".to_string(), "key-1".to_string(), pointer),
                ("legacy".to_string(), "key-1".to_string(), pointer),
            ]
        );
        assert_eq!(store.receipts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn receipt_lookup_stops_on_failure_instead_of_falling_back() {
        let store = FakeStore::new(vec![Err(Error::Forbidden("read denied".into())), Ok(Some(receipt()))]);
        let scopes = vec!["current".to_string(), "legacy".to_string()];
        let error = receipt_from_store(
            &store,
            &scopes,
            &normalize_idempotency_key("key-1").unwrap(),
            &mut NoTransaction,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Forbidden(message) if message == "read denied"));
        assert_eq!(store.calls.lock().unwrap().len(), 1);
        assert_eq!(store.receipts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn receipt_lookup_returns_none_for_empty_or_exhausted_candidates() {
        for scopes in [vec![], vec!["current".to_string(), "legacy".to_string()]] {
            let store = FakeStore::new(scopes.iter().map(|_| Ok(None)).collect());
            let result = receipt_from_store(
                &store,
                &scopes,
                &normalize_idempotency_key("key-1").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
            assert!(result.is_none());
            assert_eq!(store.calls.lock().unwrap().len(), scopes.len());
        }
    }

    #[tokio::test]
    async fn definition_lookup_preserves_opt_in_version_check_and_missing_message() {
        for (version, expected_version, accepted) in
            [(1, None, true), (2, Some(2), true), (1, Some(2), false)]
        {
            let store = FakeStore::new(vec![]);
            *store.graph.lock().unwrap() = Some(Ok(Some(graph(version))));
            let mut executor = NoTransaction;
            let pointer = &mut executor as *mut NoTransaction as usize;
            let result =
                definition_from_store(&store, &binding(), expected_version, "原入口定义缺失", &mut executor)
                    .await;
            if accepted {
                assert_eq!(result.unwrap().definition.definition_version, version);
            } else {
                assert!(matches!(result, Err(Error::ConflictError(message)) if message == "原入口定义缺失"));
            }
            assert_eq!(
                *store.calls.lock().unwrap(),
                vec![("definition-1".to_string(), "definition".to_string(), pointer)]
            );
        }
    }

    #[tokio::test]
    async fn definition_lookup_preserves_missing_and_repository_failures() {
        let store = FakeStore::new(vec![]);
        *store.graph.lock().unwrap() = Some(Ok(None));
        let error = definition_from_store(&store, &binding(), None, "原入口定义缺失", &mut NoTransaction)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "原入口定义缺失"));
        *store.graph.lock().unwrap() = Some(Err(Error::Internal("read failed".into())));
        let error = definition_from_store(&store, &binding(), Some(2), "unused", &mut NoTransaction)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "read failed"));
    }
}
