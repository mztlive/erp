//! 当前页销售单客户名称批量解析。
use std::collections::{HashMap, HashSet};

use erp_core::ids::CustomerAccountId;
use erp_customer::CustomerExt;
use erp_customer::repository::prelude::*;
use erp_party::PartyExt;
use persistence_core::NoTransaction;

use super::{SalesOrderReadService, SalesOrderView};
use crate::Result;

impl SalesOrderReadService {
    /// 批量补齐当前页客户名称，不改变列表成员与顺序。
    ///
    /// # 参数
    /// * `items` - 已完成授权分页的销售单视图
    /// # 返回
    /// 原地填写客户名称；缺失客户或主体资料时名称为空。
    /// # 错误
    /// 客户或主体仓储查询失败时返回错误。
    pub(super) async fn fill_customer_names(&self, items: &mut [SalesOrderView]) -> Result<()> {
        let ids = items.iter().map(|item| item.customer_id.clone()).collect::<Vec<_>>();
        let names = self.customer_names_batch(&ids).await?;
        for item in items {
            item.customer_name = names.get(&item.customer_id).cloned();
        }
        Ok(())
    }

    /// 只按已授权销售当前页引用的客户读取名称，不产生客户目录候选。
    ///
    /// # 参数
    /// * `customer_ids` - 当前页销售客户身份
    /// # 返回
    /// 返回现行主体名称映射；缺失客户或主体资料保持缺失。
    /// # 错误
    /// 客户或主体仓储查询失败时返回错误。
    pub(crate) async fn customer_names_batch(
        &self,
        customer_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        let ids = customer_ids
            .iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .map(CustomerAccountId::new)
            .collect::<Vec<_>>();
        let customers = self.db.customer_accounts().find_accounts_by_ids(&ids, &mut NoTransaction).await?;
        let (_, revisions) = self
            .db
            .party()
            .list_with_current_revisions(
                &customers.iter().map(|customer| customer.party_id.clone()).collect::<Vec<_>>(),
                &mut NoTransaction,
            )
            .await?;
        let names = revisions
            .into_iter()
            .map(|revision| (revision.party_id.to_string(), revision.legal_name))
            .collect::<HashMap<_, _>>();
        Ok(customers
            .into_iter()
            .filter_map(|customer| {
                names.get(customer.party_id.as_ref()).cloned().map(|name| (customer.base.id, name))
            })
            .collect())
    }
}
