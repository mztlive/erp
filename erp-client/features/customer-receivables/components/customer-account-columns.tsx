/**
 * 客户往来表格列定义入口（按行类型拆分实现，见 column-types / receivable-columns /
 * receipt-columns / invoice-columns）。本文件仅做公共出口，保持既有导入路径不变。
 */

export type { ColumnActions, CustomerAccountPreviewTarget } from "./column-types"
export { createReceivableColumns } from "./receivable-columns"
export { createReceiptColumns } from "./receipt-columns"
export { createInvoiceColumns } from "./invoice-columns"
