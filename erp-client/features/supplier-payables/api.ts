/**
 * W12 供应商往来 · 稳定 HTTP 入口。
 * 请求实现见 api/requests；DTO 映射见 api/mappers。本文件只做再导出。
 */

export {
    fetchAllocationSession,
    fetchPayableDetail,
    fetchSupplierAccounts,
    resolveUnknownResult,
    reverseInvoice,
    reversePayment,
    saveAllocationDraft,
    submitInvoice,
    submitPayment,
} from "@/features/supplier-payables/api/requests"
