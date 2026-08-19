/**
 * W12 供应商往来 · 真实 HTTP 入口。
 * 实现已按资源拆分（supplier-accounts / payments / invoices / drafts），
 * 共享会话与幂等状态见 api/shared。本文件只做再导出，保持原导入路径兼容。
 */

export {
    fetchAllocationSession,
    fetchPayableDetail,
    fetchSupplierAccounts,
    fetchSupplierPayment,
} from "@/features/supplier-payables/api/supplier-accounts"

export {
    ensureSupplierPaymentDraft,
    reversePayment,
    submitPayment,
} from "@/features/supplier-payables/api/payments"

export {
    reverseInvoice,
    submitInvoice,
} from "@/features/supplier-payables/api/invoices"

export {
    resolveUnknownResult,
    saveAllocationDraft,
} from "@/features/supplier-payables/api/drafts"

export {
    ensureSupplierRefundDraft,
    fetchSupplierRefund,
    forgetSupplierRefundDraft,
    submitSupplierRefund,
} from "@/features/supplier-payables/api/refunds"
