//! 销售导出合同号与最终销售分页共用一个快照，独立查询不复用旧阶段凭证。

use std::collections::{BTreeSet, HashMap};

use application_core::AuditActor;
use async_trait::async_trait;
use erp_contract::ContractExt;
use erp_contract::authorization::scope as contract_scope;
use erp_contract::repository::prelude::*;
use erp_identity::Error as IdentityError;
use erp_permissions::contract;
use persistence_core::{Executor, Transactional};
use serde_json::Value;

use super::{SalesListParams, SalesListView, SalesOrderReadService, read_list};
use crate::{Error, Result};

#[async_trait]
trait ExportRead: Sync {
    /// 读取并验证完整销售分页；每次调用都签发自己的原根凭证。
    async fn page(
        &self,
        params: &SalesListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesListView>;

    /// 只读取当前页真实引用且具备独立详情资格的合同号。
    async fn contracts(
        &self,
        ids: &BTreeSet<String>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>>;
}

impl SalesOrderReadService {
    /// 在原销售授权快照内读取导出分页及具备详情资格的合同号。
    ///
    /// # 参数
    /// * `params` - 与销售列表相同的筛选和跨页范围版本。
    /// * `actor` - 当前已认证操作人。
    ///
    /// # 返回
    /// 返回原销售分页 JSON；有合同引用的行追加合同号或既有无权提示。
    ///
    /// # 错误
    /// 销售无权、任何查询或交付检查失败、最终根与合同引用不一致时拒绝整页。
    pub async fn sales_order_export_page(
        &self,
        params: &SalesListParams,
        actor: &AuditActor,
    ) -> Result<Value> {
        crate::support::ensure_deep_page(params.page.unwrap_or(1), params.scope_version.as_deref())?;
        validator::Validate::validate(params)?;
        let reader = Self::new(self.db.clone(), self.authorization.clone());
        let (params, actor) = (params.clone(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { export_page(&reader, &params, &actor, executor).await })
            })
            .await
    }
}

#[async_trait]
impl ExportRead for SalesOrderReadService {
    async fn page(
        &self,
        params: &SalesListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesListView> {
        read_list(self, params, actor, executor).await
    }

    async fn contracts(
        &self,
        ids: &BTreeSet<String>,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let authorization = &self.authorization;
        let access = match authorization.query(actor, &contract::DETAIL, executor).await {
            Ok(access) => access,
            Err(IdentityError::Forbidden(_) | IdentityError::Unauthenticated(_)) => {
                return Ok(HashMap::new());
            },
            Err(error) => return Err(error.into()),
        };
        let scope = contract_scope(access, authorization.clone());
        let mut numbers = HashMap::new();
        for id in ids {
            if let Some(row) = self.db.contracts().find_authorized(id, &scope, executor).await? {
                numbers.insert(id.clone(), row.contract_no);
            }
        }
        scope.validate(executor).await?;
        Ok(numbers)
    }
}

/// 独立合同查询结束后重新授权销售根，并核对全部输出身份和合同绑定。
async fn export_page(
    reader: &impl ExportRead,
    params: &SalesListParams,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Value> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("销售导出必须使用快照事务".into()));
    }
    let initial = reader.page(params, actor, executor).await?;
    let ids = initial
        .data
        .items
        .iter()
        .filter_map(|row| row.contract_id.as_ref().filter(|id| !id.is_empty()).cloned())
        .collect();
    let numbers = reader.contracts(&ids, actor, executor).await?;
    let final_page = reader.page(params, actor, executor).await?;
    ensure_same_page(&initial, &final_page)?;
    let mut output =
        serde_json::to_value(final_page).map_err(|_| Error::Internal("销售导出列表无法编码".into()))?;
    if let Some(rows) = output["items"].as_array_mut() {
        for row in rows {
            if let Some(id) = row["contract_id"].as_str().filter(|id| !id.is_empty()) {
                let number = numbers.get(id).map(String::as_str).unwrap_or("合同资料不可读取");
                row["contract_no"] = Value::String(number.into());
            }
        }
    }
    Ok(output)
}

/// 已验证的合同号只允许附加到原销售身份、版本和引用完全一致的最终分页。
fn ensure_same_page(initial: &SalesListView, final_page: &SalesListView) -> Result<()> {
    let same_rows = initial.data.items.len() == final_page.data.items.len()
        && initial.data.items.iter().zip(&final_page.data.items).all(|(left, right)| {
            left.id == right.id && left.version == right.version && left.contract_id == right.contract_id
        });
    if initial.scope_version != final_page.scope_version
        || initial.data.total != final_page.data.total
        || initial.data.page != final_page.data.page
        || initial.data.page_size != final_page.data.page_size
        || !same_rows
    {
        return Err(Error::ConflictError("销售导出根或合同引用已变化，请重新查询".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
