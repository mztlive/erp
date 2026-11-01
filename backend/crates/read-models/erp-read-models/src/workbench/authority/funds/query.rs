//! 其余领域的往来名称、创建人与来源对手方。

use std::collections::{HashMap, HashSet};

use erp_customer::CustomerExt;
use erp_finance::entity::payable::PayableAccount;
use erp_finance::entity::receivable::ReceivableAccount;
use erp_party::{Party, PartyExt};
use erp_supplier::SupplierExt;
use persistence_core::Executor;

use super::super::amount::non_empty;
use crate::errors::Result;

impl super::super::WorkItemFactsReader {
    /// 读取应付账户上的供应商显示名。
    ///
    /// # 参数
    /// * `accounts` - 已读应付账户。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回供应商 ID 到显示名；法定名称缺失时回退供应商编号。
    ///
    /// # 错误
    /// 供应商或主体名称读取失败时返回错误。
    pub(in crate::workbench) async fn payable_supplier_names(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = accounts.iter().map(|account| account.supplier_id.to_string()).collect::<Vec<_>>();
        self.supplier_display_names(&ids, executor).await
    }

    /// 读取应付来源采购单号。
    ///
    /// # 参数
    /// * `accounts` - 已读应付账户，来源单据 ID 当作采购单 ID。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回采购单 ID 到采购单号。
    ///
    /// # 错误
    /// 采购单读取失败时返回错误。
    pub(in crate::workbench) async fn payable_purchase_numbers(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = accounts.iter().map(|account| account.source_document_id.clone()).collect::<Vec<_>>();
        Ok(self
            .read_purchase_orders(&ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.purchase_no))
            .collect())
    }

    /// 读取主体当前修订的法定名称。
    ///
    /// # 参数
    /// * `party_ids` - 主体 ID；为空时不读取。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回主体 ID 到非空法定名称；没有当前修订或名称为空的主体不出现。
    ///
    /// # 错误
    /// 主体或修订读取失败时返回错误。
    pub(in crate::workbench) async fn party_legal_names(
        &self,
        party_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let parties = self.db.parties().list_active_by_ids(party_ids, executor).await?;
        self.legal_names_for_parties(&parties, executor).await
    }

    /// 读取本批主体当前修订的法定名称。
    ///
    /// # 参数
    /// * `parties` - 本批主体
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回主体 ID 到法定名称。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn legal_names_for_parties(
        &self,
        parties: &[Party],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let revision_ids =
            parties.iter().filter_map(|party| party.stable.current_revision_id.clone()).collect::<Vec<_>>();
        if revision_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let names_by_revision = self
            .db
            .party_revisions()
            .list_active_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision.legal_name))
            .collect::<HashMap<_, _>>();
        Ok(parties
            .iter()
            .filter_map(|party| {
                let revision_id = party.stable.current_revision_id.as_ref()?;
                let name = names_by_revision.get(revision_id).cloned()?;
                non_empty(&name).map(|name| (party.base.id.clone(), name))
            })
            .collect())
    }

    /// 用主体当前法定名称解析客户显示名，没有名称时回退客户编号。
    ///
    /// # 参数
    /// * `customer_ids` - 客户账户 ID；为空时不读取。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回客户 ID 到显示名。只包含读到的有效客户。
    ///
    /// # 错误
    /// 客户或主体名称读取失败时返回错误。
    pub(in crate::workbench) async fn customer_display_names(
        &self,
        customer_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if customer_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let customers = self.db.customer_accounts().list_active_by_ids(customer_ids, executor).await?;
        let party_ids = customers.iter().map(|item| item.party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(customers
            .into_iter()
            .map(|customer| {
                let name =
                    party_names.get(&customer.party_id.to_string()).cloned().unwrap_or(customer.customer_no);
                (customer.base.id, name)
            })
            .collect())
    }

    /// 用主体当前法定名称解析供应商显示名，没有名称时回退供应商编号。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商账户 ID；为空时不读取。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回供应商 ID 到显示名。只包含读到的有效供应商。
    ///
    /// # 错误
    /// 供应商或主体名称读取失败时返回错误。
    pub(in crate::workbench) async fn supplier_display_names(
        &self,
        supplier_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if supplier_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let suppliers = self.db.supplier_accounts().list_active_by_ids(supplier_ids, executor).await?;
        let party_ids = suppliers.iter().map(|item| item.party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(suppliers
            .into_iter()
            .map(|supplier| {
                let name =
                    party_names.get(&supplier.party_id.to_string()).cloned().unwrap_or(supplier.supplier_no);
                (supplier.base.id, name)
            })
            .collect())
    }

    /// 识别应收来源修订中属于卡券销售的修订。
    ///
    /// # 参数
    /// * `accounts` - 已读应收子账；修订 ID 为空时不读取。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 返回带卡券类目 SKU 或卡券到期时间的销售修订 ID。
    ///
    /// # 错误
    /// 销售修订读取失败时返回错误。
    pub(in crate::workbench) async fn receivable_voucher_revision_ids(
        &self,
        accounts: &[ReceivableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        let revision_ids = accounts
            .iter()
            .map(|account| account.source_sales_order_revision_id.to_string())
            .collect::<Vec<_>>();
        if revision_ids.is_empty() {
            return Ok(HashSet::new());
        }
        let revisions = self.read_sales_revisions(&revision_ids, executor).await?;
        Ok(super::mapping::voucher_revision_ids(&revisions))
    }
}
