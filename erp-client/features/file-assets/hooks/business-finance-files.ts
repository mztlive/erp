"use client"
import { useMutation, useQuery } from "@tanstack/react-query"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    fetchSalesInvoiceFiles,
    fetchPurchasePaymentReceipts,
    downloadSalesInvoiceFile,
    downloadPurchasePaymentReceipt,
    type FinancialFile,
} from "../api/business-finance-files"

export function useSalesInvoiceFiles(salesOrderId: string) {
    const query = useQuery({
        queryKey: [...queryKeyRoots.salesOrders, "invoice-files", salesOrderId],
        queryFn: () => fetchSalesInvoiceFiles(salesOrderId),
        staleTime: 0,
    })
    const download = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: (file: FinancialFile) =>
            downloadSalesInvoiceFile(salesOrderId, file),
        onError: (error) =>
            toast.add({
                type: "error",
                title: "发票下载失败",
                description: getErrorMessage(error, "请重试"),
            }),
    })
    return { query, download }
}
export function usePurchasePaymentReceipts(workItemId: string) {
    const query = useQuery({
        queryKey: [
            ...queryKeyRoots.supplierPayables,
            "payment-receipts",
            workItemId,
        ],
        queryFn: () => fetchPurchasePaymentReceipts(workItemId),
        staleTime: 0,
    })
    const download = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: (file: FinancialFile) =>
            downloadPurchasePaymentReceipt(workItemId, file),
        onError: (error) =>
            toast.add({
                type: "error",
                title: "付款回单下载失败",
                description: getErrorMessage(error, "请重试"),
            }),
    })
    return { query, download }
}
