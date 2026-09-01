//! 供应商能力修订合格校验的服务编排。
//!
//! 纯业务规则已下沉至 `entities::supplier::eligibility`；本模块仅负责
//! 已持久化事实的加载与领域判定的适配，不持有可复用的校验实现。

use database::{NoTransaction, SupplierExt};
use entities::{
    common::time::BusinessDate,
    ids::{SupplierAccountId, SupplierCapabilityRevisionId},
    supplier::{
        eligibility::{
            ensure_capability_qualified as ensure_qualified_domain, CapabilityEligibilityViolation,
        },
        SupplierCapabilityRevision,
    },
};
use mongodb::Database;

use crate::errors::{Error, Result};

/// 校验指定供应商能力修订在业务日是否仍为当前启用版本。
///
/// 领域判定已由 `entities::supplier::eligibility::ensure_capability_qualified`
/// 承载；本函数仅加载供应商、能力及修订事实并将领域违例映射为服务层
/// 业务错误，保持既有 API 文案不变。
///
/// 注意：资质适用性校验（`ensure_linked_qualification`）已被人为刻意临时
/// 关闭，恢复资质数据后应解除注释恢复校验。
///
/// # 参数
/// * `db` - 数据库实例（调用方执行器，本函数不开启事务）
/// * `supplier_id` - 供应商角色 ID
/// * `capability_revision_id` - 待校验的能力修订 ID
/// * `on_date` - 业务自然日（由调用方显式注入，不读取全局时钟）
///
/// # 返回
/// 校验通过返回 `Ok(())`。
///
/// # 错误
/// * `NotFound` - 供应商不存在
/// * `BusinessLogicError` - 供应商已停用、能力不存在、版本不存在或
///   领域判定认为不合格（停用、归属不符、非当前版本、未生效、已过期）
///
/// # 约束
/// * 仅执行事实加载与领域委派，不在 Service 重复实现校验规则
/// * 不开启或提交事务；不持有全局时钟、ID 生成器或密钥
pub(crate) async fn ensure_capability_qualified(
    db: &Database,
    supplier_id: &SupplierAccountId,
    capability_revision_id: &SupplierCapabilityRevisionId,
    on_date: BusinessDate,
) -> Result<()> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let revision = load_capability_revision(db, capability_revision_id).await?;
    let capability = db
        .supplier_capabilities()
        .find_by_supplier_and_code(supplier_id, revision.capability_code, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商能力不存在".to_string()))?;

    ensure_qualified_domain(&supplier, &capability, &revision, on_date).map_err(
        |violation| match violation {
            CapabilityEligibilityViolation::SupplierDisabled => {
                Error::BusinessLogicError("供应商已停用，不能用于供给或采购".to_string())
            }
            _ => Error::BusinessLogicError("供应商能力已停用、过期或版本已变化".to_string()),
        },
    )?;

    // =========================================================================
    // 【人为刻意临时关闭】资质适用性校验（ensure_linked_qualification）：
    // 业务原因：当前阶段供应商资质数据尚未完整维护，导致供给登记被误拦截
    // （422「供应商该项能力没有适用且有效的资质，不能用于供给或采购」）。
    // 恢复策略：供应商资质数据完善后，移除此注释并恢复下述调用。
    // =========================================================================
    // ensure_linked_qualification(db, &capability.base.id, on_date).await
    Ok(())
}

/// 加载能力修订。
///
/// # 参数
/// * `db` - 数据库实例
/// * `revision_id` - 修订 ID
///
/// # 返回
/// 返回修订实体；不存在时返回业务错误。
///
/// # 错误
/// 修订不存在时返回 `BusinessLogicError("供应商能力版本不存在")`。
async fn load_capability_revision(
    db: &Database,
    revision_id: &SupplierCapabilityRevisionId,
) -> Result<SupplierCapabilityRevision> {
    db.supplier_capability_revisions()
        .find_by_id(revision_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商能力版本不存在".to_string()))
}
