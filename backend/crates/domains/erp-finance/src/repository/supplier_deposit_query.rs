//! 预存款单域查询：所有 Mongo 条件、排序和有界读取仅在拥有仓储定义。
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::{QueryModel, QueryOutput};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, aggregate, mongo_ops};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use super::funds_matching::ledger_by_external_transaction;
use crate::entity::supplier_deposit::{
    DepositLedgerKind, SupplierDepositAccount, SupplierDepositAuthorization, SupplierDepositBatch,
    SupplierDepositReservation, SupplierDepositUsage,
};
use crate::repository::{PayableExt, SupplierDepositExt};
use crate::{Error, Result};

/// 在账户范围内取得最近一百项证据；不扫描其他账户。
/// 返回按创建时间和身份倒序的最近记录。
/// 查询或反序列化失败时拒绝。
///
/// 输入约定：
/// - `repository`：当前财务记录类型的拥有仓储。
/// - `account` / `executor`：已授权账户及原执行器。
pub async fn latest_account_records<T: Serialize + DeserializeOwned + Send + Sync>(
    repository: Repository<'_, T>,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    Ok(mongo_ops::find_many(
        &repository.collection(),
        doc! {"account_id": account, "deleted_at": 0_i64},
        FindOptions::builder().sort(doc! {"created_at": -1, "id": -1}).limit(100).build(),
        executor,
    )
    .await?)
}

/// 预存款账户列表业务状态筛选。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepositAccountStatusFilter {
    /// 启用且无待核对差异。
    Available,
    /// 启用但因对账差异暂停新增使用。
    ReconciliationRequired,
    /// 已停用。
    Disabled,
}

/// 先执行 identity 编译的完整授权条件，再装载一页资金账户。
/// 结果：最多size加一条账户，额外一条用于继续分页。
/// 查询模型错配、凭证跨边界、分页非法或读取失败时拒绝。
///
/// 输入约定：
/// - `db` / `executor`：财务数据源和执行器。
/// - `supplier` / `payer` / `status`：仅收窄账户候选的筛选。
/// - `skip` / `size`：已规范化分页位置与上限。
/// - `access`：当前读取阶段由 identity 编译并复验的授权聚合阶段。
#[allow(clippy::too_many_arguments)]
pub async fn account_page(
    db: &Database,
    supplier: Option<&str>,
    payer: Option<&str>,
    status: Option<DepositAccountStatusFilter>,
    skip: u64,
    size: u32,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositAccount>> {
    let pipeline = account_pipeline(supplier, payer, status, skip, size, access, executor)?;
    let rows = account_rows(db, pipeline, executor).await?;
    access.ensure_current(executor)?;
    Ok(rows)
}

// 供应商预存款账户查询先核对固定模型、原读取边界及业务条件，再输出可执行聚合阶段。
fn account_pipeline(
    supplier: Option<&str>,
    payer: Option<&str>,
    status: Option<DepositAccountStatusFilter>,
    skip: u64,
    size: u32,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> Result<Vec<Document>> {
    account_model(access.model())?;
    access.ensure_current(executor)?;
    if size == 0 || size > 100 {
        return Err(Error::ValidationError("账户分页上限为一百条".into()));
    }

    let mut filter = doc! {"deleted_at": 0_i64};
    if let Some(supplier) = supplier {
        filter.insert("identity.supplier_id", supplier);
    }
    if let Some(payer) = payer {
        filter.insert("identity.payer_party_id", payer);
    }
    if let Some(status) = status {
        match status {
            DepositAccountStatusFilter::Available => {
                filter.insert("enabled", true);
                filter.insert("reconciliation_required", false);
            },
            DepositAccountStatusFilter::ReconciliationRequired => {
                filter.insert("enabled", true);
                filter.insert("reconciliation_required", true);
            },
            DepositAccountStatusFilter::Disabled => {
                filter.insert("enabled", false);
            },
        }
    }

    let mut pipeline = access.stages().to_vec();
    pipeline.extend([
        doc! {"$match": filter},
        doc! {"$sort": {"created_at": -1, "id": -1}},
        doc! {"$skip": i64::try_from(skip).map_err(|_| Error::ValidationError("分页范围过大".into()))?},
        doc! {"$limit": i64::from(size) + 1},
    ]);
    Ok(pipeline)
}

// 定义预存款全量账户查询可使用的固定资源、集合、行身份及对象粒度，供校验凭证与模型是否匹配。
fn account_model(model: &QueryModel) -> Result<()> {
    if model.resource != "supplier_deposit"
        || model.collection != <Database as SupplierDepositExt>::SUPPLIER_DEPOSIT_ACCOUNTS
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(Error::ValidationError("预存款查询授权模型不匹配".into()));
    }
    Ok(())
}

/// 按真实到账时间和稳定身份装载可分配批次。
/// 返回有剩余额度的FIFO批次，超过一万候选时明确拒绝。
/// 失败条件：超限或读取失败时拒绝，不能跳过早期批次。
///
/// 输入约定：
/// - `db` / `account` / `executor`：数据源、资金账户和占用事务。
pub async fn allocatable_batches(
    db: &Database,
    account: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositBatch>> {
    let filter = doc! {"account_id": account, "deleted_at": 0_i64, "$expr": {"$gt": [{"$subtract": [{"$add": ["$amount", "$returned_amount"]}, {"$add": ["$reserved_amount", "$consumed_amount", "$reversed_amount"]}]}, {"$toDecimal": "0"}]}};
    let values: Vec<SupplierDepositBatch> = mongo_ops::find_many(
        &db.supplier_deposit_batches().collection(),
        filter,
        FindOptions::builder().sort(doc! {"evidence.received_at": 1, "id": 1}).limit(10001).build(),
        executor,
    )
    .await?;
    if values.len() > 10000 {
        return Err(Error::BusinessLogicError("待分配充值批次数量过多，请先整理资金来源".into()));
    }
    Ok(values)
}

/// 读取执行组唯一原占用，不按根单或供应商名称猜测。
/// 返回原占用或缺失。
/// 查询失败时拒绝。
///
/// 输入约定：
/// - `db` / `execution` / `executor`：数据源、冻结执行组与执行器。
pub async fn reservation_for_execution(
    db: &Database,
    execution: &str,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierDepositReservation>> {
    Ok(db
        .supplier_deposit_reservations()
        .find_one_by_field("basis.execution_group_id", execution, executor)
        .await?)
}

/// 读取原占用全部资金使用记录，按稳定身份排序。
/// 返回相同执行组的使用记录。
/// 读取失败时拒绝。
///
/// 输入约定：
/// - `db` / `reservation` / `executor`：数据源、原占用与执行器。
pub async fn usages_for_reservation(
    db: &Database,
    reservation: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositUsage>> {
    Ok(db
        .supplier_deposit_usages()
        .find_many_sorted(doc! {"reservation_id": reservation}, doc! {"id": 1}, executor)
        .await?)
}

/// 返回批准采购同主体、同供应商的可选账户身份，不读取余额作为批准资格。
/// 返回已启用人民币账户候选。
/// 读取失败时拒绝。
///
/// 输入约定：
/// - `db` / `payer` / `supplier` / `executor`：原采购批准来源及执行器。
pub async fn eligible_accounts(
    db: &Database,
    payer: &str,
    supplier: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositAccount>> {
    let values: Vec<SupplierDepositAccount> = mongo_ops::find_many(&db.supplier_deposit_accounts().collection(), doc! {"identity.payer_party_id": payer, "identity.supplier_id": supplier, "identity.currency": "CNY", "enabled": true, "deleted_at": 0_i64}, FindOptions::builder().sort(doc! {"created_at": 1, "id": 1}).limit(101).build(), executor).await?;
    if values.len() > 100 {
        return Err(Error::BusinessLogicError("可选预存款账户超过上限，请先核查账号配置".into()));
    }
    Ok(values)
}

/// 防止历史银行付款的同一流水再次登记为充值现金来源。
/// 结果：相同主体已存在该银行流水时返回真，包括历史普通采购付款。
/// 失败条件：查询失败时拒绝，不将未知当作不存在。
///
/// 输入约定：
/// - `db` / `payer` / `reference` / `executor`：原主体、银行流水及同一财务事务。
pub async fn bank_source_exists(
    db: &Database,
    payer: &str,
    reference: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    Ok(db
        .supplier_payments()
        .exists(doc! {"payer_party_id": payer, "bank_reference": reference}, executor)
        .await?)
}

/// 读取原人工命令的永久执行组占用，当前采购退役不影响原结果恢复。
/// 返回最多一千项原命令占用，按稳定身份排序。
/// 前缀无效、超限或读取失败时拒绝。
///
/// 输入约定：
/// - `db` / `account` / `request`：原财务范围。
/// - `prefix`：服务器命令身份形成的完整操作前缀。
/// - `executor`：原恢复执行器。
pub async fn reservations_for_manual_command(
    db: &Database,
    account: &str,
    request: &str,
    prefix: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositReservation>> {
    if prefix.is_empty() || prefix.len() > 256 {
        return Err(Error::ValidationError("余额执行命令前缀无效".into()));
    }

    let mut expression = String::from("^");
    for character in prefix.chars() {
        if ".+*?()[]{}|^$\\".contains(character) {
            expression.push('\\');
        }
        expression.push(character);
    }

    let values: Vec<SupplierDepositReservation> = mongo_ops::find_many(&db.supplier_deposit_reservations().collection(), doc! {
        "account_id":account, "basis.payment_request_id":request, "operation_key":{"$regex":expression}, "deleted_at":0_i64,
    }, FindOptions::builder().sort(doc! {"id":1}).limit(1001).build(), executor).await?;
    if values.len() > 1000 {
        return Err(Error::BusinessLogicError("原余额执行组超过上限，请核查原命令".into()));
    }
    Ok(values)
}

/// 读取当前账户有效期限内的自动授权；人工单次授权不进入自动候选。
/// 返回稳定排序的有限自动授权候选，业务限额仍由授权实体校验。
/// 超过一千候选或读取失败时拒绝。
///
/// 输入约定：
/// - `db` / `account` / `now` / `executor`：账户、服务器时间和原执行器。
pub async fn automatic_authorizations(
    db: &Database,
    account: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositAuthorization>> {
    let values: Vec<SupplierDepositAuthorization> = mongo_ops::find_many(&db.supplier_deposit_authorizations().collection(), doc! {
        "account_id":account,"revoked":false,"valid_from":{"$lte":now.unix_secs()},"valid_until":{"$gt":now.unix_secs()},
        "$or":[{"mode.type":"automatic"},{"mode":{"$exists":false}}], "deleted_at":0_i64,
    }, FindOptions::builder().sort(doc! {"valid_until":1,"id":1}).limit(1001).build(), executor).await?;
    if values.len() > 1000 {
        return Err(Error::BusinessLogicError("账户自动授权过多，请先核查授权配置".into()));
    }
    Ok(values)
}

#[derive(Deserialize)]
struct LedgerBalanceRow {
    #[serde(rename = "_id")]
    kind: DepositLedgerKind,
    amount: Amount,
}
/// 从真实不可变资金流水重建截止时点余额，拒绝客户端自报本地余额。
/// 返回已保存实际资金来源形成的定点余额。
/// 未知资金方向、金额溢出或聚合失败时拒绝。
///
/// 输入约定：
/// - `db` / `account` / `cutoff` / `executor`：原账户、截止时点和对账事务。
pub async fn ledger_balance_at(
    db: &Database,
    account: &str,
    cutoff: Instant,
    executor: &mut dyn Executor,
) -> Result<Amount> {
    let collection = db.supplier_deposit_ledger().collection();
    let pipeline = vec![
        doc! {"$match": {"account_id": account, "evidence.occurred_at": {"$lte": cutoff.unix_secs()}, "deleted_at": 0_i64}},
        doc! {"$group": {"_id": "$kind", "amount": {"$sum": "$amount"}}},
    ];
    let rows = aggregate::rows(&collection.clone_with_type::<LedgerBalanceRow>(), pipeline, executor).await?;

    rows.into_iter().try_fold(Amount::zero(), |balance, row| {
        let decimal = match row.kind {
            DepositLedgerKind::Recharge | DepositLedgerKind::Return => {
                balance.to_decimal().checked_add(row.amount.to_decimal())
            },
            DepositLedgerKind::Debit | DepositLedgerKind::RechargeReversal => {
                balance.to_decimal().checked_sub(row.amount.to_decimal())
            },
        }
        .ok_or_else(|| Error::BusinessLogicError("资金流水余额重建溢出".into()))?;
        Amount::try_from(decimal).map_err(Into::into)
    })
}

async fn account_rows(
    db: &Database,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierDepositAccount>> {
    let collection = db.supplier_deposit_accounts().collection();
    Ok(aggregate::rows(&collection, pipeline, executor).await?)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application_core::AuditActor;
    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::entity::authorization::{
        AccessContext, AccessFact, FactKind, QueryModelKey, RelationKey,
    };
    use erp_identity::ports::{AccessMapping, QueryFragment, RelationMapping};
    use erp_identity::testing::fixed_authorization;
    use erp_identity::{AuthorizationService, Error as IdentityError, Permission, Result as IdentityResult};
    use mongodb::Client;
    use mongodb::bson::Bson;
    use persistence_core::ReadExecutor;

    use super::*;

    struct Mapping(QueryModel);

    #[async_trait]
    impl AccessMapping for Mapping {
        fn model(&self) -> &QueryModel {
            &self.0
        }

        fn key(&self, _: &AccessContext) -> IdentityResult<Bson> {
            Ok("$id".into())
        }

        fn records(&self, _: &AccessContext) -> IdentityResult<QueryFragment> {
            Ok(QueryFragment::Match(doc! {"deleted_at": 0_i64}))
        }

        fn predicate(&self, _: &AccessFact, _: &AccessContext) -> IdentityResult<QueryFragment> {
            Ok(QueryFragment::Match(doc! {"owner_id": "user"}))
        }

        fn relation(&self, _: &RelationKey, _: &AccessContext) -> IdentityResult<RelationMapping> {
            Err(IdentityError::ValidationError("账户没有来源关系".into()))
        }
    }

    fn model() -> QueryModel {
        QueryModel {
            key: QueryModelKey::parse("supplier_deposit_accounts").unwrap(),
            resource: "supplier_deposit".into(),
            collection: "supplier_deposit_accounts".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [
                FactKind::Owner,
                FactKind::Organization,
                FactKind::Collaborators,
                FactKind::SettlementParty,
            ]
            .into(),
        }
    }

    async fn access(model: QueryModel, executor: &mut dyn Executor) -> CompiledAccess {
        let permission = Permission::parse("supplier_deposit:list").unwrap();
        let service: Arc<AuthorizationService> = fixed_authorization(
            std::slice::from_ref(&permission),
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            vec![],
            vec![(permission.clone(), Arc::new(Mapping(model)))],
        )
        .unwrap();
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        service.query(&actor, &permission, executor).await.unwrap()
    }

    #[test]
    fn account_model_requires_fixed_resource_collection_identity_and_object_output() {
        account_model(&model()).unwrap();
        for invalid in [
            QueryModel { resource: "supplier".into(), ..model() },
            QueryModel { collection: "other_accounts".into(), ..model() },
            QueryModel { id_field: "account_id".into(), ..model() },
            QueryModel { output: QueryOutput::Share { fields: ["id".into()].into() }, ..model() },
            QueryModel { output: QueryOutput::Existence { fields: ["id".into()].into() }, ..model() },
        ] {
            assert!(matches!(account_model(&invalid), Err(Error::ValidationError(_))));
        }
    }

    #[tokio::test]
    async fn account_filters_preserve_all_authorization_stages_before_pagination() {
        let mut executor = ReadExecutor::default();
        let access = access(model(), &mut executor).await;
        let pipeline = account_pipeline(
            Some("supplier"),
            Some("payer"),
            Some(DepositAccountStatusFilter::ReconciliationRequired),
            3,
            7,
            &access,
            &mut executor,
        )
        .unwrap();
        let prefix = access.stages().len();
        assert_eq!(&pipeline[..prefix], access.stages());
        assert_eq!(
            pipeline[prefix],
            doc! {"$match": {
                "deleted_at": 0_i64,
                "identity.supplier_id": "supplier",
                "identity.payer_party_id": "payer",
                "enabled": true,
                "reconciliation_required": true
            }}
        );
        assert_eq!(pipeline[prefix + 2], doc! {"$skip": 3_i64});
        assert_eq!(pipeline[prefix + 3], doc! {"$limit": 8_i64});
    }

    #[tokio::test]
    async fn account_page_rejects_wrong_models_and_stale_boundaries_before_reading() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("deposit_query_guard");
        let mut executor = ReadExecutor::default();
        for invalid in [
            QueryModel { collection: "other_accounts".into(), ..model() },
            QueryModel { id_field: "account_id".into(), ..model() },
        ] {
            let access = access(invalid, &mut executor).await;
            assert!(matches!(
                account_page(&db, None, None, None, 0, 10, &access, &mut executor).await,
                Err(Error::ValidationError(_))
            ));
        }

        let access = access(model(), &mut executor).await;
        assert!(matches!(
            account_page(&db, None, None, None, 0, 10, &access, &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
        executor.next_read();
        assert!(matches!(
            account_page(&db, None, None, None, 0, 10, &access, &mut executor).await,
            Err(Error::ValidationError(_))
        ));
    }
}
