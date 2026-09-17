use erp_core::ids::{PartyId, SupplierAccountId};
use persistence_core::{Executor, Result};

use super::super::{SUPPLIER_QUALIFICATION_CAPABILITIES, SupplierRepository};
use super::SupplierDetailBundle;
use crate::entity::supplier::SupplierCommercialProfileRevision;
use crate::repository::owned::SupplierQualificationCapabilityRepository;

impl<'a> SupplierRepository<'a> {
    /// 批量加载供应商详情所需的全部事实（`PROC-R04`）。
    ///
    /// 一次调用返回供应商、主体当前指针及联系人、地址、税务、银行、能力、
    /// 资质关联、评级与商务版本历史；资质与能力关联一次批量读取，查询次数
    /// 有固定上界，不随资质或能力数量增长。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 供应商不存在时返回 `None`；存在时返回全部事实束，主体缺失、当前指针
    /// 缺失或历史集合为空时以 `None`/空集合表达，由 Service 映射为明确语义。
    ///
    /// # 错误
    /// 任一批量仓储查询失败时返回错误。
    ///
    /// # 约束
    /// 不开启或提交事务；不记录敏感明文日志；不返回 Service DTO 或 View。
    pub async fn load_supplier_detail_bundle(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierDetailBundle>> {
        let Some(supplier) = self.account(supplier_id, executor).await? else {
            return Ok(None);
        };
        let party_id = supplier.party_id.clone();
        let capabilities = self.list_capabilities(supplier_id, executor).await?;
        let qualifications = self.list_qualifications(supplier_id, executor).await?;
        let qualification_ids: Vec<erp_core::ids::SupplierQualificationId> = qualifications
            .iter()
            .map(|item| erp_core::ids::SupplierQualificationId::new(&item.base.id))
            .collect();
        let qualification_links =
            SupplierQualificationCapabilityRepository::new(self.db, SUPPLIER_QUALIFICATION_CAPABILITIES)
                .list_by_qualification_ids(&qualification_ids, executor)
                .await?;
        let ratings = self.list_ratings_latest_first(supplier_id, executor).await?;
        let commercial_profiles = self.list_commercial_profiles_latest_first(supplier_id, executor).await?;
        let commercial_party_ids = commercial_party_ids(&commercial_profiles);
        Ok(Some(SupplierDetailBundle {
            supplier,
            party_id,
            capabilities,
            qualifications,
            qualification_links,
            ratings,
            commercial_profiles,
            commercial_party_ids,
        }))
    }
}

/// 收集商务版本引用的签约与付款主体 ID。
///
/// 转调 Service 侧 `list_view::commercial_party_ids` 唯一实现
/// （erp-supplier-005），语义一致；仓储内不再保留第二份去重逻辑。
fn commercial_party_ids(profiles: &[SupplierCommercialProfileRevision]) -> Vec<PartyId> {
    crate::service::supplier::commercial_party_ids_for_repository(profiles)
}
