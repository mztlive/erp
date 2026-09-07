//! File-asset adapter for financial review evidence validation.

use erp_core::common::time::Instant;
use erp_finance::entity::receivable::{ReviewEvidenceAssetFact, ValidatedCardFundsReviewDecision};
use erp_support::FileAsset;
use services::{Error, Result};

pub(super) use erp_finance::service::receivable::card_funds_decision::{
    canonical_evidence, validated_from_dto, workflow_comment,
};

/// Translate support-owned asset usability into finance evidence facts.
///
/// Evaluates usability at the supplied point in time and preserves missing-evidence
/// ordering and business error categories. No additional reads or clock access occur.
pub fn validate_evidence_assets(
    validated: &ValidatedCardFundsReviewDecision,
    assets: &[FileAsset],
    now: Instant,
) -> Result<()> {
    let facts = assets
        .iter()
        .map(|asset| ReviewEvidenceAssetFact {
            id: asset.base.id.clone().into(),
            usable: asset.is_usable_at(now),
        })
        .collect::<Vec<_>>();
    erp_finance::service::receivable::card_funds_decision::validate_evidence_assets(validated, &facts)
        .map_err(Error::from)
}
