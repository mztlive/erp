//! 组合根按固定顺序登记各领域索引，索引定义仍由领域唯一拥有。

/// 创建全部域依赖的幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure_indexes(db: &mongodb::Database) -> persistence_core::Result<()> {
    erp_identity::indexes::ensure_accounts_and_roles(db).await?;
    erp_audit::indexes::ensure(db).await?;
    erp_identity::indexes::ensure_authorization(db).await?;
    erp_workflow::indexes::ensure_approval_integration(db).await?;
    erp_workflow::indexes::ensure_bpm(db).await?;
    erp_support::indexes::ensure_bulk_job(db).await?;
    erp_catalog::indexes::ensure(db).await?;
    erp_contract::indexes::ensure(db).await?;
    erp_finance::indexes::ensure_cost(db).await?;
    erp_customer::indexes::ensure(db).await?;
    erp_workflow::indexes::ensure_document_registry(db).await?;
    erp_support::indexes::ensure_file_asset(db).await?;
    erp_fulfillment::indexes::ensure(db).await?;
    erp_integration::indexes::ensure(db).await?;
    erp_inventory::indexes::ensure(db).await?;
    erp_import::indexes::ensure(db).await?;
    erp_party::indexes::ensure(db).await?;
    erp_finance::indexes::ensure_payable(db).await?;
    erp_procurement::indexes::ensure(db).await?;
    erp_finance::indexes::ensure_receivable(db).await?;
    erp_returns::indexes::ensure(db).await?;
    erp_sales::indexes::ensure(db).await?;
    erp_support::indexes::ensure_source_registry(db).await?;
    erp_supplier::indexes::ensure(db).await?;
    erp_supply::indexes::ensure(db).await?;
    erp_warehouse::indexes::ensure(db).await?;
    erp_workflow::indexes::ensure_work_item(db).await?;
    Ok(())
}
