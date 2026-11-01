//! 已完成业务授权后的财务附件列表读取。
use erp_core::ids::BusinessDocumentId;
use erp_support::FileAssetExt;
use erp_support::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use super::{FinancialFileView, readable_file};
use crate::Result;

/// 使用调用方的授权事务读取可用附件，不暴露存储地址。
/// # 参数
/// 数据库、已授权单据身份、展示单号及当前执行器。
/// # 返回
/// 文件展示清单。
/// # 错误
/// 附件或资产读取失败时返回错误。
pub async fn financial_document_files(
    db: &Database,
    id: &str,
    number: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<FinancialFileView>> {
    let attachments =
        db.document_attachments().list_by_document(&BusinessDocumentId::new(id), executor).await?;
    let ids = attachments.into_iter().map(|row| row.file_asset_id).collect::<Vec<_>>();
    let files = db.file_assets().find_by_ids(&ids, executor).await?;
    Ok(files
        .into_iter()
        .filter(readable_file)
        .map(|file| FinancialFileView::new(id.into(), number.into(), file))
        .collect())
}
