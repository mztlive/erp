//! 采购提交同一事务沿冻结来源重验；不猜测历史供给，不刷新冻结价格或版本。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use erp_core::ids::{SupplierAccountId, SupplierOfferingId};
use erp_procurement::entity::facts::{AvailabilityFact, OfferingFact, OfferingRevisionFact};
use erp_procurement::entity::purchase_order::{LineSupply, PurchaseOrderSubmissionLine};
use erp_read_models::purchase_center::repository::{
    availability_fact, offering_fact, offering_revision_fact,
};
use erp_supply::entity::supplier_offering::OfferingStatus;
use erp_supply::repository::{
    SupplierOfferingAvailabilityRepositoryExt, SupplierOfferingExt, SupplierOfferingRepositoryExt,
    SupplierOfferingRevisionRepositoryExt,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 最终提交事务读取的启用供给、当前条款和独立可供事实。
struct SelectedSupplyFacts {
    offerings: HashMap<String, OfferingFact>,
    revisions: HashMap<String, OfferingRevisionFact>,
    availability: HashMap<String, AvailabilityFact>,
}

/// 以冻结主键读取最新三种事实后委托采购领域核对，不改写采购行。
///
/// # 参数
/// `db` 为组合层数据库；`supplier_id`、`lines` 为本次冻结提交；`executor` 为启动事务执行器。
/// # 返回
/// 所有已记录正式选源的行仍可提交时成功；来源未知的历史行保留未知。
/// # 错误
/// 精确来源缺失、停止/暂停、版本或数量变化、条款失效时返回重核选源冲突。
pub(super) async fn revalidate_selected_supply(
    db: &Database,
    supplier_id: &SupplierAccountId,
    lines: &[PurchaseOrderSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = lines
        .iter()
        .filter_map(|line| line.supplier_offering_source.as_ref())
        .map(|source| source.supplier_offering_id.clone())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(());
    }
    let facts = load_selected_facts(db, &ids, executor).await?;
    for line in lines {
        let Some(source) = &line.supplier_offering_source else { continue };
        let offering = facts.offerings.get(source.supplier_offering_id.as_ref()).ok_or_else(changed)?;
        let revision_id = offering.stable.current_revision_id.as_deref().ok_or_else(changed)?;
        let revision = facts.revisions.get(revision_id).ok_or_else(changed)?;
        let availability =
            facts.availability.get(source.supplier_offering_id.as_ref()).ok_or_else(changed)?;
        let supply = LineSupply {
            offering: offering.clone(),
            revision: revision.clone(),
            availability: availability.clone(),
        };
        source
            .ensure_current_supply(
                &supply,
                supplier_id,
                line.sku_id.as_ref().ok_or_else(changed)?,
                line.quantity.ok_or_else(changed)?,
                BusinessDate::today(),
            )
            .map_err(|_| changed())?;
    }
    Ok(())
}

/// 三次批量查询只沿精确供给主键和当前条款指针，不按供应商及 SKU 寻找替代供给。
async fn load_selected_facts(
    db: &Database,
    ids: &[SupplierOfferingId],
    executor: &mut dyn Executor,
) -> Result<SelectedSupplyFacts> {
    let offerings = db
        .supplier_offerings()
        .list_by_ids(ids, executor)
        .await?
        .into_iter()
        .filter(|offering| offering.stable.status == OfferingStatus::Active)
        .map(offering_fact)
        .collect::<Vec<_>>();
    let revision_ids = offerings
        .iter()
        .filter_map(|offering| offering.stable.current_revision_id.clone())
        .collect::<Vec<_>>();
    let revisions = db.supplier_offering_revisions().list_by_ids(&revision_ids, executor).await?;
    let availability = db.supplier_offering_availabilities().find_by_offering_ids(ids, executor).await?;
    Ok(SelectedSupplyFacts {
        offerings: offerings.into_iter().map(|fact| (fact.base.id.clone(), fact)).collect(),
        revisions: revisions
            .into_iter()
            .map(offering_revision_fact)
            .map(|fact| (fact.base.id.clone(), fact))
            .collect(),
        availability: availability
            .into_iter()
            .map(availability_fact)
            .map(|fact| (fact.supplier_offering_id.to_string(), fact))
            .collect(),
    })
}

/// 同一稳定冲突要求重新核对；不泄露外域当前版本或静默替换旧事实。
fn changed() -> Error {
    Error::ConflictError("采购供给、条款或可供事实已变化，请重新核对选源后提交".into())
}
