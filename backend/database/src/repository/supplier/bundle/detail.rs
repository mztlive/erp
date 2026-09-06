use crate::repository::owned::SupplierQualificationCapabilityRepository;
use std::collections::{HashMap, HashSet};

use entities::supplier::SupplierCommercialProfileRevision;
use erp_core::ids::{PartyId, SupplierAccountId};

use super::super::super::extensions::PartyExt;
use super::super::{SupplierRepository, SUPPLIER_QUALIFICATION_CAPABILITIES};
use super::SupplierDetailBundle;
use persistence_core::Executor;
use persistence_core::Result;

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
        let party_bundle = self
            .db
            .party()
            .find_with_current_revision(&supplier.party_id, executor)
            .await?;
        let (party, party_revision) = match party_bundle {
            Some((party, revision)) => (Some(party), revision),
            None => (None, None),
        };
        let contacts = self
            .db
            .party_contacts()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let addresses = self
            .db
            .party_addresses()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let tax_profiles = self
            .db
            .party_tax_profiles()
            .list_by_party(&supplier.party_id, executor)
            .await?;
        let bank_accounts = self
            .db
            .party_bank_accounts()
            .list_by_party(&supplier.party_id, executor)
            .await?;
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
        let commercial_profiles = self
            .list_commercial_profiles_latest_first(supplier_id, executor)
            .await?;
        let commercial_party_names = self
            .commercial_party_names(&commercial_profiles, executor)
            .await?;
        Ok(Some(SupplierDetailBundle {
            supplier,
            party,
            party_revision,
            contacts,
            addresses,
            tax_profiles,
            bank_accounts,
            capabilities,
            qualifications,
            qualification_links,
            ratings,
            commercial_profiles,
            commercial_party_names,
        }))
    }

    /// 批量读取商务版本引用的签约与付款主体当前法定名称。
    ///
    /// # 参数
    /// * `profiles` - 已加载的商务资料历史
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回主体 ID 字符串到当前法定名称的映射；缺少当前修订时无键。
    ///
    /// # 错误
    /// 主体或当前修订批量查询失败时返回错误。
    ///
    /// # 约束
    /// 只经主体域属主访问器组装，不在供应商仓储内直查外域集合。
    async fn commercial_party_names(
        &self,
        profiles: &[SupplierCommercialProfileRevision],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let party_ids: Vec<PartyId> = profiles
            .iter()
            .flat_map(|profile| {
                [
                    profile.signing_entity_party_id.to_string(),
                    profile.payment_entity_party_id.to_string(),
                ]
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .map(PartyId::new)
            .collect();
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        self.db
            .party()
            .current_legal_names_by_party_ids(&party_ids, executor)
            .await
    }
}
