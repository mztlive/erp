"use client"

import { useMutation, useQuery } from "@tanstack/react-query"
import { toast } from "@/components/ui/toast"
import type { FinancialFile } from "@/features/file-assets/api/business-finance-files"
import { getErrorMessage } from "@/lib/api/errors"
import { purchaseOrderKeys } from "./queries"
import {
    downloadPurchasePaymentReceipt,
    fetchPurchasePaymentReceipt,
    fetchPurchasePayments,
} from "../api/purchase-payments"

export function usePurchasePayments(purchaseOrderId: string) {
    const query = useQuery({
        queryKey: [...purchaseOrderKeys.detail(purchaseOrderId), "payments"],
        queryFn: () => fetchPurchasePayments(purchaseOrderId),
        staleTime: 0,
    })
    const download = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: (file: FinancialFile) =>
            downloadPurchasePaymentReceipt(purchaseOrderId, file),
        onError: (error) =>
            toast.add({
                type: "error",
                title: "付款回单下载失败",
                description: getErrorMessage(error, "请重试"),
            }),
    })
    return { query, download }
}

export function usePurchasePaymentReceipt(
    purchaseOrderId: string,
    paymentId: string,
) {
    return useQuery({
        queryKey: [
            ...purchaseOrderKeys.detail(purchaseOrderId),
            "payments",
            paymentId,
            "receipt",
        ],
        queryFn: () => fetchPurchasePaymentReceipt(purchaseOrderId, paymentId),
        staleTime: 0,
        gcTime: 0,
    })
}
