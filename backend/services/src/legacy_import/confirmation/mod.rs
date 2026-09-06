mod complete;
mod create;
mod query;

#[cfg(test)]
mod tests;

#[cfg(test)]
use complete::{
    confirmation_command_identity, confirmation_completion_receipt_message, confirmation_result_status,
    parse_confirmation_completion_receipt, validate_confirmation_completion, ConfirmationCompletionReceipt,
};
#[cfg(test)]
use create::{
    collect_superseded_closable_work_items, confirmation_next_step, replaced_confirmation_work_item_ids,
};
#[cfg(test)]
use query::{append_confirmation_actions, read_only_work_item_view};

#[cfg(test)]
use entities::legacy_import::{
    confirmation_work_item, ConfirmationDecision, ConfirmationMatrixDecision, ConfirmationStatus,
    LegacyImportBatch, LegacyImportBatchStatus, LegacyImportConfirmation,
};
#[cfg(test)]
use entities::work_item::{WorkItem, WorkItemCloseData};
#[cfg(test)]
use erp_core::common::time::Instant;

#[cfg(test)]
use crate::work_item::WorkItemAllowedAction;

#[cfg(test)]
use super::dto::{
    CreateLegacyImportConfirmationRequest, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, PreparedConfirmationCompletion,
};
