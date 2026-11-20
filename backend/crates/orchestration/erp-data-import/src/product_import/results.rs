//! 向原提交人交付按状态筛选的商品导入源行与结果。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_catalog::PRODUCT_IMPORT_HEADERS;
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BackgroundJobId, BackgroundJobItemRow, BulkJobExt, ItemStatusFilter};
use persistence_core::Transactional;
use serde::Serialize;

use super::ProductImportProcess;
use super::execute::JobRowSource;
use super::views::item_result_summary;
use super::{legacy, submit};
use crate::{Error, Result};

/// 某一执行状态的商品导入行和对应的模板表头。
#[derive(Serialize)]
pub struct ProductImportResults {
    /// 与任务源行列序一致的原模板表头。
    pub headers: &'static [&'static str],
    /// 按 Excel 行号排列的所选状态明细。
    pub rows: Vec<ProductImportResultRow>,
}

/// 用于核对的商品源行、源行号和执行说明。
#[derive(Serialize)]
pub struct ProductImportResultRow {
    /// 原工作表行号。
    pub source_row_no: u32,
    /// 源单元格文本，图片对象不随行交付。
    pub cells: Vec<String>,
    /// 行结果需要先核对实际写入情况。
    pub outcome_unknown: bool,
    /// 脱敏后的逐行说明。
    pub result_summary: Option<String>,
}

impl ProductImportProcess {
    /// 查询已结束任务指定分类的所有源行和处理结果。
    /// 读取来源前后重验任务归属和当前商品导入资格。
    /// 无权、任务未结束、源行不完整或读取失败时返回错误。
    pub async fn result_rows(
        &self,
        id: &str,
        status: ItemStatusFilter,
        actor: &AuditActor,
    ) -> Result<ProductImportResults> {
        let access = self.job_access.clone();
        let id = id.to_owned();
        let reader = actor.clone();
        let job = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let job = access.load(&id, &reader, executor).await?;
                    ensure_download(&job, &reader)?;
                    submit::authorize(&access.service, &reader, executor).await?;
                    Ok::<_, Error>(job)
                })
            })
            .await?;
        let source = self.load_row_source(&job).await?;

        let db = self.db.clone();
        let access = self.job_access.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let current = access.reload(&job, &actor, executor).await?;
                    submit::authorize(&access.service, &actor, executor).await?;
                    let page_size = u32::try_from(current.total_count.max(1))
                        .map_err(|_| Error::Internal("导入任务行数无效".into()))?;
                    let items = db
                        .background_job_items()
                        .search_job_items(
                            &BackgroundJobId::new(&current.base.id),
                            Some(status),
                            1,
                            page_size,
                            executor,
                        )
                        .await?;
                    if usize::try_from(items.total) != Ok(items.items.len()) {
                        return Err(Error::Internal("导入任务明细不完整".into()));
                    }
                    export_rows(source, items.items)
                })
            })
            .await
    }
}

// 源文件含报价等数据，只交付已结束任务的原提交人。
fn ensure_download(job: &BackgroundJob, actor: &AuditActor) -> Result<()> {
    if job.requested_by != actor.id() {
        return Err(Error::Forbidden("仅任务发起人可下载商品导入行".into()));
    }
    if job.finished_at.is_none() {
        return Err(Error::ConflictError("请等待任务结束后下载导入结果".into()));
    }
    Ok(())
}

// 使用 Excel 行号匹配明细，保留空行间隔和历史模板的价格列含义。
fn export_rows(source: JobRowSource, items: Vec<BackgroundJobItemRow>) -> Result<ProductImportResults> {
    let mut source_rows = match source {
        JobRowSource::Manifest(manifest) => {
            manifest.rows.into_iter().map(|row| (row.row_number, row.cells)).collect::<HashMap<_, _>>()
        },
        JobRowSource::Legacy { parsed, .. } => {
            parsed.rows.into_iter().map(|row| (row.row_number, row.cells)).collect()
        },
    };
    let width = items
        .first()
        .and_then(|item| source_rows.get(&item.source_row_no.unwrap_or(item.item_no)))
        .map_or(PRODUCT_IMPORT_HEADERS.len(), Vec::len);
    let headers = match width {
        width if width == PRODUCT_IMPORT_HEADERS.len() => PRODUCT_IMPORT_HEADERS,
        legacy::SUPPLY_COLUMNS => &PRODUCT_IMPORT_HEADERS[..legacy::SUPPLY_COLUMNS],
        width if width == legacy::COMPACT_HEADERS.len() => legacy::COMPACT_HEADERS,
        _ => return Err(Error::Internal("导入源数据的模板列数无效".into())),
    };

    let rows = items
        .into_iter()
        .map(|item| {
            let source_row_no = item.source_row_no.unwrap_or(item.item_no);
            let cells =
                source_rows.remove(&source_row_no).filter(|cells| cells.len() == width).ok_or_else(|| {
                    Error::BusinessLogicError(format!("Excel 第 {source_row_no} 行的源数据无法读取"))
                })?;
            Ok(ProductImportResultRow {
                source_row_no,
                cells,
                outcome_unknown: item.result_code.as_deref() == Some("outcome_unknown"),
                result_summary: item_result_summary(&item),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ProductImportResults { headers, rows })
}

#[cfg(test)]
mod tests {
    use erp_support::ItemStatus;

    use super::*;
    use crate::product_import::row_manifest::{RowManifest, RowManifestRow};

    fn source(width: usize) -> JobRowSource {
        JobRowSource::Manifest(RowManifest {
            version: 4,
            request_id: "request".into(),
            rows: [2, 8]
                .map(|row_number| RowManifestRow {
                    row_number,
                    cells: vec![format!("000{row_number}"); width],
                    ..Default::default()
                })
                .into(),
        })
    }

    fn failed_item(row_number: u32) -> BackgroundJobItemRow {
        BackgroundJobItemRow {
            id: "item".into(),
            background_job_id: "job".into(),
            item_no: 2,
            object_type: None,
            object_id: None,
            worksheet_name: Some("对内".into()),
            source_row_no: Some(row_number),
            status: Some(ItemStatus::Failed),
            result_code: Some("outcome_unknown".into()),
            result_summary: Some("请核对商品资料".into()),
            result_object_type: None,
            result_object_id: None,
        }
    }

    #[test]
    fn export_matches_selected_items_by_excel_row_for_current_and_legacy_layouts() {
        for headers in [
            PRODUCT_IMPORT_HEADERS,
            &PRODUCT_IMPORT_HEADERS[..legacy::SUPPLY_COLUMNS],
            legacy::COMPACT_HEADERS,
        ] {
            let export = export_rows(source(headers.len()), vec![failed_item(8)]).unwrap();
            assert_eq!(export.headers, headers);
            assert_eq!(export.rows.len(), 1);
            let row = &export.rows[0];
            assert_eq!(row.source_row_no, 8);
            assert_eq!(row.cells[0], "0008");
            assert!(row.outcome_unknown);
            assert_eq!(row.result_summary.as_deref(), Some("请核对商品资料"));
        }
    }

    #[test]
    fn export_reports_missing_source_row_instead_of_shifting_rows() {
        assert!(matches!(
            export_rows(source(PRODUCT_IMPORT_HEADERS.len()), vec![failed_item(9)]),
            Err(Error::BusinessLogicError(message)) if message.contains("第 9 行")
        ));
    }
}
