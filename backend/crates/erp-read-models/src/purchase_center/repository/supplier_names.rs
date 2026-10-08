//! 把供应商账号身份解析为当前法定名称的剩余调用点辅助。

use std::collections::HashMap;

use erp_core::ids::{PartyId, SupplierAccountId};
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use mongodb::Database;
use persistence_core::{Executor, Result};

/// 按供应商账号组装当前法定名称；缺失的主体或修订不进入结果。
///
/// # 参数
/// * `db` - 目标数据库。
/// * `supplier_ids` - 供应商账号身份。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回供应商账号身份到当前法定名称的映射；主体或修订缺失的账号省略。
///
/// # 错误
/// 供应商引用或主体名称读取失败时返回对应错误。
pub async fn current_legal_names_by_account_ids(
    db: &Database,
    supplier_ids: &[SupplierAccountId],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, String>> {
    let refs = db.supplier().supplier_party_ids_by_account_ids(supplier_ids, executor).await?;
    let party_ids: Vec<PartyId> = refs.values().cloned().collect();
    let party_names = db.party().current_legal_names_by_party_ids(&party_ids, executor).await?;
    Ok(refs
        .into_iter()
        .filter_map(|(supplier_id, party_id)| {
            party_names.get(&party_id.to_string()).cloned().map(|legal_name| (supplier_id, legal_name))
        })
        .collect())
}
