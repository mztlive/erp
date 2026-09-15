//! 销售选品：选品册、陈列、会话、方案与准备任务。

mod booklet;
mod combination;
mod display_item;
mod idempotency;
mod image;
mod limits;
mod pool;
mod pool_member;
mod prepare_task;
mod pricing;
mod proposal;
mod session;
mod sku_snapshot;
mod status;
mod tier;
mod token;
mod types;

pub use booklet::{SalesSelectionBooklet, SalesSelectionBookletData};
pub use combination::{GeneratedPackage, TierSearchReport, combination_key, search_packages};
pub use display_item::{DisplayKind, SalesSelectionDisplayItem, ensure_publishable_display};
pub use idempotency::{
    IdempotencyOperation, SalesSelectionIdempotency, SalesSelectionIdempotencyData, request_hash,
};
pub use image::{FirstNonEmptyMemberImage, PackageCoverRef, PackageImageGenerator};
pub use limits::*;
pub use pool::{PoolFilterSnapshot, PoolSource};
pub use pool_member::SalesSelectionPoolMember;
pub use prepare_task::{SalesSelectionPrepareTask, SalesSelectionPrepareTaskData};
pub use pricing::{abs_diff, try_add, try_mul_u32, try_sum};
pub use proposal::{
    SalesSelectionProposal, SalesSelectionProposalData, SalesSelectionProposalDisplayLine,
    SalesSelectionProposalSkuLine, build_proposal_lines,
};
pub use session::{SalesSelectionSession, SessionChoice};
pub use sku_snapshot::{ImageAssetSnapshot, SkuSnapshot, SpecificationAttributeSnapshot, sort_by_sku_id};
pub use status::BookletStatus;
pub use tier::TierRule;
pub use token::{LinkTokenCrypto, token_hash};
pub use types::{
    PoolSourceKind, PrepareKind, PrepareStage, PrepareTaskStatus, ProposalSource, SearchStopReason,
    SelectionForm, SubmitMode, normalize_idempotency_key,
};
