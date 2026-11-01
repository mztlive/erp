//! 账户、到账凭证、执行组、外部交易和对账证据的永久唯一约束。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::SupplierDepositExt;

/// 登记供应商预存款集合索引，不删除历史资金证据。
/// # 参数
/// * `db` - 财务数据库。
/// # 返回
/// 全部唯一和查询索引创建成功。
/// # 错误
/// 历史重复或数据库失败时拒绝启动。
pub async fn ensure(db: &Database) -> Result<()> {
    for (collection, definitions) in definitions() {
        let mut indexes = vec![index("uk_id", doc! {"id": 1}, true)];
        indexes.extend(definitions.into_iter().map(|(name, keys, unique)| index(name, keys, unique)));
        db.collection::<Document>(collection).create_indexes(indexes).await?;
    }
    super::funds_evidence::ensure(db).await
}

type IndexDefinition = (&'static str, Document, bool);
type CollectionDefinition = (&'static str, Vec<IndexDefinition>);

fn definitions() -> Vec<CollectionDefinition> {
    let mut definitions = account_definitions();
    definitions.extend(execution_definitions());
    definitions.extend(resolution_definitions());
    definitions
}

fn account_definitions() -> Vec<CollectionDefinition> {
    vec![
        (
            Database::SUPPLIER_DEPOSIT_RECHARGE_REQUESTS,
            vec![("uk_deposit_recharge_request_operation", doc! {"account_id": 1, "operation_key": 1}, true)],
        ),
        (
            Database::SUPPLIER_DEPOSIT_ACCOUNTS,
            vec![
                (
                    "idx_deposit_candidate_scope",
                    doc! {"identity.payer_party_id": 1, "identity.supplier_id": 1, "identity.currency": 1, "enabled": 1, "id": 1},
                    false,
                ),
                (
                    "uk_deposit_identity",
                    doc! {"identity.payer_party_id": 1, "identity.supplier_id": 1, "identity.external_account_ref": 1, "identity.currency": 1},
                    true,
                ),
            ],
        ),
        (
            Database::SUPPLIER_DEPOSIT_BATCHES,
            vec![
                ("uk_deposit_bank_payment", doc! {"evidence.supplier_payment_id": 1}, true),
                ("uk_deposit_received", doc! {"account_id": 1, "evidence.external_transaction_id": 1}, true),
                ("idx_deposit_batch_fifo", doc! {"account_id": 1, "evidence.received_at": 1, "id": 1}, false),
            ],
        ),
        (
            Database::SUPPLIER_DEPOSIT_AUTHORIZATIONS,
            vec![("idx_deposit_authorization", doc! {"account_id": 1, "revoked": 1}, false)],
        ),
    ]
}

fn execution_definitions() -> Vec<CollectionDefinition> {
    vec![
        (
            Database::SUPPLIER_DEPOSIT_RESERVATIONS,
            vec![
                ("uk_deposit_operation", doc! {"account_id": 1, "operation_key": 1}, true),
                ("uk_deposit_execution", doc! {"basis.execution_group_id": 1}, true),
                ("idx_deposit_request", doc! {"basis.payment_request_id": 1}, false),
                ("idx_deposit_outstanding", doc! {"account_id":1,"created_at":1,"id":1}, false),
            ],
        ),
        (
            Database::SUPPLIER_DEPOSIT_LEDGER,
            vec![
                (
                    "uk_deposit_external_transaction",
                    doc! {"account_id": 1, "evidence.external_transaction_id": 1},
                    true,
                ),
                ("uk_deposit_ledger_operation", doc! {"account_id": 1, "operation_key": 1}, true),
                (
                    "idx_deposit_ledger_account_time",
                    doc! {"account_id": 1, "evidence.occurred_at": 1, "id": 1},
                    false,
                ),
            ],
        ),
        (
            Database::SUPPLIER_DEPOSIT_USAGES,
            vec![
                ("uk_deposit_usage_ledger", doc! {"ledger_entry_id": 1}, true),
                ("idx_deposit_usage_reservation", doc! {"reservation_id": 1}, false),
            ],
        ),
        (
            Database::SUPPLIER_DEPOSIT_SETTLEMENTS,
            vec![
                ("uk_deposit_settlement_operation", doc! {"account_id": 1, "operation_key": 1}, true),
                ("idx_deposit_settlement_usage", doc! {"usage_id": 1, "occurred_at": 1}, false),
            ],
        ),
    ]
}

fn resolution_definitions() -> Vec<CollectionDefinition> {
    vec![(
        Database::SUPPLIER_DEPOSIT_RECONCILIATIONS,
        vec![
            ("uk_deposit_reconciliation", doc! {"account_id":1,"operation_key":1}, true),
            ("idx_deposit_reconciliation_range", doc! {"account_id":1,"covered_until":-1}, false),
            (
                "idx_deposit_reconciliation_open",
                doc! {"account_id":1,"resolved_by":1,"covered_from":1,"id":1},
                false,
            ),
        ],
    )]
}

fn index(name: &str, keys: Document, unique: bool) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.to_string()).unique(unique).build())
        .build()
}
