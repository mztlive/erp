//! 企业身份唯一性用例步骤；沿用调用方的写入事务。

use mongodb::Database;
use persistence_core::Executor;

use crate::repository::identity_guard::guarded_candidates;
use crate::{Error, Party, PartyKind, PartyRevision, Result};

/// 为新建或修订企业证明当前身份没有被其他主体占用。
///
/// # 参数
/// * `db` / `party` / `revision` - 数据库、目标主体及拟生效名称修订。
/// * `executor` - 原写入事务。
/// # 返回
/// 无其他主体命中时成功；非企业沿用其原有身份规则。
/// # 错误
/// 名称或信用代码已占用、并发写入或数据库失败时拒绝。
pub async fn ensure_identity_available(
    db: &Database,
    party: &Party,
    revision: &PartyRevision,
    executor: &mut dyn Executor,
) -> Result<()> {
    if party.party_kind != PartyKind::Enterprise {
        return Ok(());
    }
    let candidates =
        guarded_candidates(db, &revision.legal_name, party.unified_credit_code.as_deref(), executor).await?;
    if candidates.iter().any(|candidate| candidate.party.base.id != party.base.id) {
        return Err(Error::ConflictError("该企业名称或信用代码已建档，请复用已有主体，不能重复创建".into()));
    }
    Ok(())
}
