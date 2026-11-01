//! 原始资金证据身份、冲突、固定窗口和余额观察的永久索引。

use mongodb::bson::{Document, doc};
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index, unique_partial_index};

use crate::repository::FundsEvidenceExt;

pub(super) async fn ensure(db: &Database) -> Result<()> {
    let definitions = [
        (
            Database::FUNDS_SCAN_LEASES,
            vec![
                unique_index("uk_funds_lease_id", doc! {"id":1}),
                unique_index("uk_funds_lease_account", doc! {"account_id":1}),
            ],
        ),
        (Database::FUNDS_EVIDENCE_INBOX, inbox_indexes()),
        (
            Database::FUNDS_EVIDENCE_CONFLICTS,
            vec![
                unique_index("uk_funds_conflict_id", doc! {"id":1}),
                named_index("idx_funds_conflict_inbox", doc! {"account_id":1,"inbox_id":1,"id":1}),
            ],
        ),
        (Database::FUNDS_SCAN_CHECKPOINTS, scan_indexes()),
        (
            Database::FUNDS_BALANCE_OBSERVATIONS,
            vec![
                unique_index("uk_funds_balance_id", doc! {"id":1}),
                named_index(
                    "idx_funds_balance_observed",
                    doc! {"account_id":1,"observation.observed_at":-1,"id":-1},
                ),
            ],
        ),
    ];
    for (collection, indexes) in definitions {
        db.collection::<Document>(collection).create_indexes(indexes).await?;
    }
    Ok(())
}

fn inbox_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_funds_evidence_id", doc! {"id":1}),
        unique_index(
            "uk_funds_evidence_source",
            doc! {"account_id":1,"source_key.kind":1,"source_key.value":1},
        ),
        named_index(
            "idx_funds_evidence_pending",
            doc! {"account_id":1,"state":1,"input.source_observed_at":1,"id":1},
        ),
        named_index(
            "idx_funds_evidence_order",
            doc! {"account_id":1,"input.result.external_order_id":1,"input.source_observed_at":1,"id":1},
        ),
    ]
}

fn scan_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_funds_scan_id", doc! {"id":1}),
        unique_index("uk_funds_scan_key", doc! {"account_id":1,"scan_key":1}),
        unique_partial_index("uk_funds_scan_active_account", doc! {"account_id":1}, doc! {"finished":false}),
        named_index(
            "idx_funds_scan_latest",
            doc! {"account_id":1,"finished":1,"completed_through":-1,"created_at":-1,"id":-1},
        ),
        named_index("idx_funds_scan_coverage", doc! {"account_id":1,"window.since":1,"window.through":1}),
    ]
}
