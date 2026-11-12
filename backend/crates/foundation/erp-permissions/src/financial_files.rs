//! financial_files 的内置权限。
use crate::{Permission, receivable, sales_order, work_item};

/// 查看采购履约付款进度。
pub const FULFILLMENT_PAYMENT_PURCHASE_PAYMENT_SUMMARY: Permission = work_item::DETAIL;

/// 查询发票文件。
pub const INVOICE_INVOICE_FILES: Permission = receivable::INVOICE_DETAIL;

/// 下载发票文件。
pub const INVOICE_INVOICE_DOWNLOAD: Permission = receivable::INVOICE_DETAIL;

/// 下载本单发票文件。
pub const SALES_INVOICE_FILES: Permission = sales_order::DETAIL;

/// 下载本单发票文件。
pub const SALES_INVOICE_DOWNLOAD: Permission = sales_order::DETAIL;

/// 下载采购履约付款回单。
pub const PURCHASE_PAYMENT_RECEIPTS: Permission = work_item::DETAIL;

/// 下载采购履约付款回单。
pub const PURCHASE_PAYMENT_RECEIPT_DOWNLOAD: Permission = work_item::DETAIL;

/// 查询回款凭证。
pub const RECEIPT_RECEIPT_FILES: Permission = receivable::CUSTOMER_RECEIPT_DETAIL;

/// 下载回款凭证。
pub const RECEIPT_RECEIPT_FILE_DOWNLOAD: Permission = receivable::CUSTOMER_RECEIPT_DETAIL;

/// 查询回款凭证。
pub const RECEIPT_RECEIPT_DRAFT_FILES: Permission = receivable::CUSTOMER_RECEIPT_SUBMIT;

/// 下载回款凭证。
pub const RECEIPT_RECEIPT_DRAFT_FILE_DOWNLOAD: Permission = receivable::CUSTOMER_RECEIPT_SUBMIT;

/// 本模块全部复用其他模块的权限定义。
pub const ENTRIES: &[(&str, crate::PermissionDef)] = &[];
