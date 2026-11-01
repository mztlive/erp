//! 已授权资金行补上主体与供应商的法定名称。名称缺失不阻断列表。

use std::collections::HashMap;

use erp_core::ids::{PartyId, SupplierAccountId};
use erp_party::PartyExt;
use erp_party::repository::prelude::*;
use erp_supplier::SupplierExt;
use erp_supplier::repository::prelude::*;
use persistence_core::Executor;

use super::authorization::FundsAccess;
use crate::{Error, Result};

impl FundsAccess {
    /// 按主体当前修订解析法定名称。
    ///
    /// # 参数
    /// * `party_ids` - 主体主键；空集合不访问数据库
    /// * `executor` - 调用方事务
    ///
    /// # 返回
    /// 主体 id 到非空法定名称。没有当前修订的主体不进入映射。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    pub(super) async fn party_legal_names(
        &self,
        party_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let unique = crate::support::dedup_sorted(party_ids.iter().filter(|id| !id.is_empty()).cloned());
        if unique.is_empty() {
            return Ok(HashMap::new());
        }
        let keys = unique.into_iter().map(PartyId::new).collect::<Vec<_>>();
        let parties = self.db.parties().find_parties_by_ids(&keys, executor).await.map_err(Error::from)?;
        let revision_ids =
            parties.iter().filter_map(|party| party.stable.current_revision_id.clone()).collect::<Vec<_>>();
        let revisions = self
            .db
            .party_revisions()
            .find_revisions_by_ids(&revision_ids, executor)
            .await
            .map_err(Error::from)?;
        let mut legal_by_revision = HashMap::new();
        for revision in revisions {
            let name = revision.legal_name.trim().to_string();
            if !name.is_empty() {
                legal_by_revision.insert(revision.base.id.clone(), name);
            }
        }
        let mut names = HashMap::new();
        for party in parties {
            let Some(revision_id) = party.stable.current_revision_id.as_ref() else {
                continue;
            };
            let Some(name) = legal_by_revision.get(revision_id) else {
                continue;
            };
            names.insert(party.base.id.clone(), name.clone());
        }
        Ok(names)
    }

    /// 按供应商账号解析当前主体法定名称。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商账号主键
    /// * `executor` - 调用方事务
    ///
    /// # 返回
    /// 供应商 id 到非空法定名称。账号或主体缺失时不进入映射。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    pub(super) async fn supplier_legal_names(
        &self,
        supplier_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let unique = crate::support::dedup_sorted(supplier_ids.iter().filter(|id| !id.is_empty()).cloned());
        if unique.is_empty() {
            return Ok(HashMap::new());
        }
        let keys = unique.into_iter().map(SupplierAccountId::new).collect::<Vec<_>>();
        let suppliers =
            self.db.supplier_accounts().find_accounts_by_ids(&keys, executor).await.map_err(Error::from)?;
        let party_ids = suppliers.iter().map(|supplier| supplier.party_id.to_string()).collect::<Vec<_>>();
        let legal = self.party_legal_names(&party_ids, executor).await?;
        let mut names = HashMap::new();
        for supplier in suppliers {
            let Some(name) = legal.get(&supplier.party_id.to_string()) else {
                continue;
            };
            names.insert(supplier.base.id.clone(), name.clone());
        }
        Ok(names)
    }

    /// 解析单个供应商的法定名称。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商账号主键
    /// * `executor` - 调用方事务
    ///
    /// # 返回
    /// 非空法定名称；账号或主体缺失时返回 `None`。
    ///
    /// # 错误
    /// 仓储读取失败时返回错误。
    pub(super) async fn supplier_name_of(
        &self,
        supplier_id: &impl ToString,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let id = supplier_id.to_string();
        Ok(self.supplier_legal_names(std::slice::from_ref(&id), executor).await?.remove(&id))
    }
}
