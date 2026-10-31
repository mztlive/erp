//! financial_files 的接口权限目录。
use super::{PermissionDef, groups, receivable, sales_order, work_item};

/// 查看采购履约付款进度。
pub const FULFILLMENT_PAYMENT_PURCHASE_PAYMENT_SUMMARY: PermissionDef =
    work_item::DETAIL.described(groups::FINANCIAL_FILES, "查看采购履约付款进度");

/// 查询发票文件。
pub const INVOICE_INVOICE_FILES: PermissionDef =
    receivable::INVOICE_DETAIL.described(groups::INVOICE, "查询发票文件");

/// 下载发票文件。
pub const INVOICE_INVOICE_DOWNLOAD: PermissionDef =
    receivable::INVOICE_DETAIL.described(groups::INVOICE, "下载发票文件");

/// 下载本单发票文件。
pub const SALES_INVOICE_FILES: PermissionDef =
    sales_order::DETAIL.described(groups::SALES_ORDER, "下载本单发票文件");

/// 下载本单发票文件。
pub const SALES_INVOICE_DOWNLOAD: PermissionDef =
    sales_order::DETAIL.described(groups::SALES_ORDER, "下载本单发票文件");

/// 下载采购履约付款回单。
pub const PURCHASE_PAYMENT_RECEIPTS: PermissionDef =
    work_item::DETAIL.described(groups::FINANCIAL_FILES, "下载采购履约付款回单");

/// 下载采购履约付款回单。
pub const PURCHASE_PAYMENT_RECEIPT_DOWNLOAD: PermissionDef =
    work_item::DETAIL.described(groups::FINANCIAL_FILES, "下载采购履约付款回单");

/// 查询回款凭证。
pub const RECEIPT_RECEIPT_FILES: PermissionDef =
    receivable::CUSTOMER_RECEIPT_DETAIL.described(groups::INVOICE, "查询回款凭证");

/// 下载回款凭证。
pub const RECEIPT_RECEIPT_FILE_DOWNLOAD: PermissionDef =
    receivable::CUSTOMER_RECEIPT_DETAIL.described(groups::INVOICE, "下载回款凭证");

/// 查询回款凭证。
pub const RECEIPT_RECEIPT_DRAFT_FILES: PermissionDef =
    receivable::CUSTOMER_RECEIPT_SUBMIT.described(groups::INVOICE, "查询回款凭证");

/// 下载回款凭证。
pub const RECEIPT_RECEIPT_DRAFT_FILE_DOWNLOAD: PermissionDef =
    receivable::CUSTOMER_RECEIPT_SUBMIT.described(groups::INVOICE, "下载回款凭证");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("FULFILLMENT_PAYMENT_PURCHASE_PAYMENT_SUMMARY", FULFILLMENT_PAYMENT_PURCHASE_PAYMENT_SUMMARY),
    ("INVOICE_INVOICE_FILES", INVOICE_INVOICE_FILES),
    ("INVOICE_INVOICE_DOWNLOAD", INVOICE_INVOICE_DOWNLOAD),
    ("SALES_INVOICE_FILES", SALES_INVOICE_FILES),
    ("SALES_INVOICE_DOWNLOAD", SALES_INVOICE_DOWNLOAD),
    ("PURCHASE_PAYMENT_RECEIPTS", PURCHASE_PAYMENT_RECEIPTS),
    ("PURCHASE_PAYMENT_RECEIPT_DOWNLOAD", PURCHASE_PAYMENT_RECEIPT_DOWNLOAD),
    ("RECEIPT_RECEIPT_FILES", RECEIPT_RECEIPT_FILES),
    ("RECEIPT_RECEIPT_FILE_DOWNLOAD", RECEIPT_RECEIPT_FILE_DOWNLOAD),
    ("RECEIPT_RECEIPT_DRAFT_FILES", RECEIPT_RECEIPT_DRAFT_FILES),
    ("RECEIPT_RECEIPT_DRAFT_FILE_DOWNLOAD", RECEIPT_RECEIPT_DRAFT_FILE_DOWNLOAD),
];
