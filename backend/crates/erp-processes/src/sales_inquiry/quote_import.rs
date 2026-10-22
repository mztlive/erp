//! 回填报价按任务原子导入；文件身份不授予权限，事务内重新核验需求及供应关系。
use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_supply::inquiry::{InquiryQuoteScope, InquirySupplyService};
use erp_workflow::entity::work_item::WorkItem;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::SalesInquiryProcess;
use super::dto::InquiryMutationView;
use super::execution::InquiryCommand;
use super::internal_quote::{InquiryQuoteInput, SaveInquiryQuoteCommand};
use crate::{Error, Result};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedInquiryQuote {
    pub source: String,
    pub sku_id: String,
    pub quote: InquiryQuoteInput,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportInquiryQuotesCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub task_id: String,
    pub task_version: u64,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub rows: Vec<ImportedInquiryQuote>,
}

impl ImportInquiryQuotesCommand {
    fn validate(&self) -> Result<()> {
        if self.rows.is_empty() || self.rows.len() > 1000 {
            return Err(Error::ValidationError("每次须导入 1 至 1000 条供应商回复".into()));
        }
        let mut seen = BTreeSet::new();
        for row in &self.rows {
            if row.source.trim().is_empty() || row.source.chars().count() > 256 {
                return Err(Error::ValidationError("导入来源文件及行号缺失或过长".into()));
            }
            if !seen.insert((row.quote.line_id.as_str(), row.quote.supplier_id.as_ref())) {
                return Err(Error::ValidationError(format!(
                    "{}：同一供应商的商品回复重复，请只保留一份",
                    row.source
                )));
            }
            if !row.quote.terms.unavailable && row.quote.terms.freight_gross.is_none() {
                return Err(Error::ValidationError(format!("{}：请填写运费，已含运费填写 0", row.source)));
            }
        }
        Ok(())
    }
}

impl SalesInquiryProcess {
    /// 原子保存多份回填报价，复用采购任务权限和原命令恢复。
    /// # 参数
    /// `actor` 为采购，`id` 为询价，`request` 为预览后确认的数据。
    /// # 返回
    /// 返回安全命令回执，不向销售公开内部成本。
    /// # 错误
    /// 任一报价、版本或权限失效时拒绝整批，不保存部分成功数据。
    pub async fn import_internal_quotes(
        &self,
        actor: &AuditActor,
        id: &str,
        request: ImportInquiryQuotesCommand,
    ) -> Result<InquiryMutationView> {
        request.validate()?;
        self.execute(actor, id, InquiryCommand::ImportQuotes(Box::new(request))).await
    }

    pub(super) async fn append_imported_quotes(
        &self,
        actor: &AuditActor,
        inquiry: &SalesInquiry,
        task: &WorkItem,
        request: &ImportInquiryQuotesCommand,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        inquiry.ensure_processing(request.expected_version)?;
        if request.requirement_version != inquiry.requirement_version
            || inquiry.feedback_round.checked_add(1) != Some(request.feedback_round)
        {
            return Err(Error::ConflictError("报价表对应的需求或反馈轮次已变化，请重新生成报价表".into()));
        }
        let prior = InquirySupplyService::new(self.db.clone())
            .quotes(&inquiry.base.id, inquiry.requirement_version, executor)
            .await?;
        for row in &request.rows {
            let line = inquiry.responsibility_line(task.responsibility_scope_ids(), &row.quote.line_id)?;
            if line.sku_id != row.sku_id {
                return Err(Error::ValidationError(format!("{}：商品与原询价行不一致", row.source)));
            }
            let terms =
                row.quote.terms.for_requirement(line).map_err(|error| row_error(&row.source, error))?;
            let scope = InquiryQuoteScope {
                inquiry_id: inquiry.base.id.clone(),
                requirement_version: inquiry.requirement_version,
                line_id: line.line_id.clone(),
            };
            if prior.iter().any(|old| {
                old.matches_reply(
                    &scope,
                    request.feedback_round,
                    &row.quote.supplier_id,
                    &row.quote.supplier_offering_id,
                    &terms,
                    &row.quote.internal_note,
                )
            }) {
                continue;
            }
            let command = SaveInquiryQuoteCommand {
                command_key: request.command_key.clone(),
                expected_version: request.expected_version,
                task_id: request.task_id.clone(),
                task_version: request.task_version,
                quote: row.quote.clone(),
            };
            self.append_internal_quote(actor, inquiry, task, &command, executor)
                .await
                .map_err(|error| row_error(&row.source, error))?;
        }
        Ok(())
    }
}

fn row_error(source: &str, error: Error) -> Error {
    match error {
        Error::ValidationError(message) => Error::ValidationError(format!("{source}：{message}")),
        Error::BusinessLogicError(message) => Error::BusinessLogicError(format!("{source}：{message}")),
        Error::ConflictError(message) => Error::ConflictError(format!("{source}：{message}")),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn request() -> ImportInquiryQuotesCommand {
        serde_json::from_value(json!({
            "command_key":"import", "expected_version":1, "task_id":"task", "task_version":1,
            "requirement_version":1, "feedback_round":1,
            "rows":[{"source":"甲报价表.xlsx 第5行", "sku_id":"sku", "quote":{
                "quote_id":null,"revision_no":1,"line_id":"line","supplier_id":"supplier",
                "supplier_offering_id":"offering", "internal_note":"",
                "terms":{"unit_cost_gross":"10","input_tax_rate":"0.13","available_quantity":"10",
                    "expected_delivery_date":"2026-10-25","valid_to":null,"freight_gross":"0"}
            }}]
        }))
        .unwrap()
    }

    #[test]
    fn batch_accepts_complete_rows_and_rejects_duplicates_missing_freight_and_empty_batch() {
        let mut input = request();
        input.validate().unwrap();
        input.rows.push(input.rows[0].clone());
        assert!(input.validate().is_err());
        input.rows.pop();
        input.rows[0].quote.terms.freight_gross = None;
        assert!(input.validate().is_err());
        input.rows.clear();
        assert!(input.validate().is_err());
    }
}
