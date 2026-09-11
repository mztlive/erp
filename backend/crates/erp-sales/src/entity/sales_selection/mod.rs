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
pub use combination::{combination_key, search_packages, GeneratedPackage, TierSearchReport};
pub use display_item::{ensure_publishable_display, DisplayKind, SalesSelectionDisplayItem};
pub use idempotency::{
    request_hash, IdempotencyOperation, SalesSelectionIdempotency, SalesSelectionIdempotencyData,
};
pub use image::{FirstNonEmptyMemberImage, PackageCoverRef, PackageImageGenerator};
pub use limits::*;
pub use pool::{PoolFilterSnapshot, PoolSource};
pub use pool_member::SalesSelectionPoolMember;
pub use prepare_task::{SalesSelectionPrepareTask, SalesSelectionPrepareTaskData};
pub use pricing::{abs_diff, try_add, try_mul_u32, try_sum};
pub use proposal::{
    build_proposal_lines, SalesSelectionProposal, SalesSelectionProposalData,
    SalesSelectionProposalDisplayLine, SalesSelectionProposalSkuLine,
};
pub use session::{SalesSelectionSession, SessionChoice};
pub use sku_snapshot::{sort_by_sku_id, ImageAssetSnapshot, SkuSnapshot, SpecificationAttributeSnapshot};
pub use status::BookletStatus;
pub use tier::TierRule;
pub use token::{token_hash, LinkTokenCrypto};
pub use types::{
    normalize_idempotency_key, PoolSourceKind, PrepareKind, PrepareStage, PrepareTaskStatus, ProposalSource,
    SearchStopReason, SelectionForm, SubmitMode,
};
