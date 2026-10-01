//! 应收范围行补上核销需要的主体名称和分录。

use std::collections::HashMap;

use erp_core::ids::ReceivableAccountId;
use erp_finance::dto::receivable::ReceivableEntryView;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use persistence_core::Executor;

use super::authorization::FundsAccess;
use super::rows::ScopedReceivableAccountRow;
use crate::{Error, Result};

impl FundsAccess {
    /// 给已授权应收行补上往来名称和分录。
    ///
    /// # 参数
    /// * `rows` - 当前页或详情行；空切片不访问数据库
    /// * `executor` - 调用方事务
    ///
    /// # 错误
    /// 主体或分录读取失败时返回错误。
    pub(super) async fn finish_receivable_display(
        &self,
        rows: &mut [ScopedReceivableAccountRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        self.finish_receivable_names(rows, executor).await?;
        self.attach_receivable_entries(rows, executor).await
    }

    /// 补齐已授权行的主体名称，分录由调用方本次快照装载。
    ///
    /// # 参数
    /// * `rows` - 当前页或详情行；空切片不访问数据库
    /// * `executor` - 调用方事务
    ///
    /// # 返回
    /// 名称补齐后返回空值。
    ///
    /// # 错误
    /// 主体名称读取失败时返回错误。
    pub(super) async fn finish_receivable_names(
        &self,
        rows: &mut [ScopedReceivableAccountRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let party_ids = rows.iter().map(|row| row.counterparty_party_id.clone()).collect::<Vec<_>>();
        let names = self.party_legal_names(&party_ids, executor).await?;
        for row in rows.iter_mut() {
            row.counterparty_party_name = names.get(&row.counterparty_party_id).cloned();
        }
        Ok(())
    }

    /// 批量挂上应收分录。没有分录时回款核销池是空的。
    ///
    /// # 参数
    /// * `rows` - 已裁剪的应收行
    /// * `executor` - 调用方事务
    ///
    /// # 错误
    /// 分录读取失败时返回错误。
    async fn attach_receivable_entries(
        &self,
        rows: &mut [ScopedReceivableAccountRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = rows.iter().map(|row| ReceivableAccountId::new(row.id.clone())).collect::<Vec<_>>();
        let found = self
            .db
            .receivable_entries()
            .find_entries_by_accounts(&ids, executor)
            .await
            .map_err(Error::from)?;
        let mut grouped: HashMap<String, Vec<ReceivableEntryView>> = HashMap::new();
        for entry in found {
            grouped.entry(entry.receivable_account_id.to_string()).or_default().push(entry_view(entry));
        }
        for views in grouped.values_mut() {
            views.sort_by_key(|view| view.source_sequence);
        }
        for row in rows {
            row.entries = grouped.remove(&row.id).unwrap_or_default();
        }
        Ok(())
    }
}

/// 把分录实体收成范围响应。冲减合计不在这条读取里重算，核销池使用分录金额。
pub(super) fn entry_view(entry: erp_finance::entity::receivable::ReceivableEntry) -> ReceivableEntryView {
    ReceivableEntryView {
        id: entry.base.id,
        entry_type: entry.entry_type,
        direction: entry.direction,
        amount: entry.amount,
        due_date: entry.due_date,
        source_document_id: entry.source_document_id,
        source_sequence: entry.source_sequence,
        posted_at: entry.posted_at,
        offset_total: erp_finance::service::receivable::mapping::zero_amount(),
    }
}
