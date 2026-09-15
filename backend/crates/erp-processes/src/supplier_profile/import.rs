//! 逐行创建供应商。每行复用原子根命令，失败不影响其他完整行。
use application_core::AuditActor;
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use erp_supplier::dto::import::{SupplierImportRequest, SupplierImportResult, SupplierImportRow};
use persistence_core::NoTransaction;

use super::SupplierProfileService;
use crate::{Error, Result};

impl SupplierProfileService {
    /// 执行有界导入并返回每一行结果；不持久化上传中的明文敏感资料。
    ///
    /// # Errors
    /// 批次为空或超过 500 行时拒绝整个请求；业务行错误在结果中返回。
    pub async fn import(
        &self,
        request: SupplierImportRequest,
        actor: &AuditActor,
    ) -> Result<Vec<SupplierImportResult>> {
        if request.rows.is_empty() || request.rows.len() > 500 {
            return Err(Error::ValidationError("每次导入应为 1–500 行".into()));
        }
        let mut results = Vec::with_capacity(request.rows.len());
        for row in request.rows {
            let result = self.import_row(&row, actor).await;
            results.push(match result {
                Ok(value) => value,
                Err(error) => failed_row(&row, error),
            });
        }
        Ok(results)
    }

    /// 首先回读去重记录，随后完成引用解析和根命令事务。
    async fn import_row(&self, row: &SupplierImportRow, actor: &AuditActor) -> Result<SupplierImportResult> {
        row.validate()?;
        if let Some(done) = self.command_result(&row.command_key()).await? {
            return Ok(row_result(
                row,
                "skipped",
                "供应商已导入，本行未重复写入",
                Some(done.supplier_id),
                Some(done.supplier_no),
            ));
        }
        if let Some(existing) = self.existing_import_supplier(row.cell(1)).await? {
            return Ok(row_result(
                row,
                "skipped",
                "系统已有同名供应商，本行未写入",
                Some(existing.base.id),
                Some(existing.supplier_no),
            ));
        }
        let (signing, payment) = row.company_names();
        let signing = self.import_company(signing).await?;
        let payment = self.import_company(payment).await?;
        let command = row.command(signing, payment)?;
        self.submit_import_row(row, command, actor).await
    }

    /// 复用逐行根命令；失败后回读原命令以核对可能已成功的提交。
    async fn submit_import_row(
        &self,
        row: &SupplierImportRow,
        command: erp_supplier::SaveSupplierProfileRequest,
        actor: &AuditActor,
    ) -> Result<SupplierImportResult> {
        match self.create(command, actor).await {
            Ok(done) => Ok(row_result(
                row,
                "succeeded",
                "供应商已导入",
                Some(done.supplier_id),
                Some(done.supplier_no),
            )),
            Err(error) => {
                if let Some(done) = self.command_result(&row.command_key()).await? {
                    return Ok(row_result(
                        row,
                        "skipped",
                        "供应商已导入，本行未重复写入",
                        Some(done.supplier_id),
                        Some(done.supplier_no),
                    ));
                }
                Err(error)
            },
        }
    }

    /// 公司名称或别名只允许精确匹配已启用的我方公司。
    async fn import_company(&self, name: &str) -> Result<erp_core::ids::PartyId> {
        let company = self
            .db
            .parties()
            .company_by_name(name, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("公司主体“{name}”尚未维护或已停用")))?;
        Ok(erp_core::ids::PartyId::new(company.base.id))
    }

    /// 精确匹配当前法定名称，不将其他企业主体自动转换成供应商。
    async fn existing_import_supplier(&self, name: &str) -> Result<Option<erp_supplier::SupplierAccount>> {
        let parties = self.db.party().exact_current_party_ids_by_name(name, &mut NoTransaction).await?;
        for party in parties {
            if let Some(supplier) =
                self.db.supplier_accounts().find_by_party(&party, &mut NoTransaction).await?
            {
                return Ok(Some(supplier));
            }
        }
        Ok(None)
    }
}

/// 结果响应只含业务定位与结果，拒绝返回原始请求。
fn row_result(
    row: &SupplierImportRow,
    status: &str,
    message: &str,
    supplier_id: Option<String>,
    supplier_no: Option<String>,
) -> SupplierImportResult {
    SupplierImportResult {
        row_number: row.row_number,
        name: row.cell(1).into(),
        status: status.into(),
        message: message.into(),
        supplier_id,
        supplier_no,
    }
}

/// 对无法确认的提交保留独立状态，禁止误报为已回滚失败。
fn failed_row(row: &SupplierImportRow, error: Error) -> SupplierImportResult {
    let (status, message) = match error {
        Error::ValidationError(message)
        | Error::BusinessLogicError(message)
        | Error::ConflictError(message)
        | Error::NotFound(message) => ("failed", message),
        Error::Logic(error) => ("failed", error.to_string()),
        Error::OutcomeUnknown(_) => {
            ("uncertain", "提交结果暂无法确认，请使用同一文件重试核对，系统会检查已导入记录".into())
        },
        _ => ("uncertain", "暂无法确认导入结果，请重试核对".into()),
    };
    row_result(row, status, &message, None, None)
}
