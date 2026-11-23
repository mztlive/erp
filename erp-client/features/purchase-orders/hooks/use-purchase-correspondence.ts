"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    fetchPurchasePaper,
    sendPurchaseEmail,
    fetchPurchaseEmails,
} from "../api/purchase-correspondence"

export function usePurchasePaper(id: string, supplierName: string) {
    return useMutation({
        mutationFn: async () => {
            const file = await fetchPurchasePaper(id)
            const { downloadPurchasePaperImage } =
                await import("../lib/download-purchase-paper-image")
            return downloadPurchasePaperImage(file.html, supplierName)
        },
        retry: false,
    })
}
export function useSendPurchaseEmail(id: string) {
    const cache = useQueryClient()
    return useMutation({
        onSettled: () =>
            cache.invalidateQueries({
                queryKey: ["purchase-orders", id, "emails"],
            }),
        mutationFn: (request: Parameters<typeof sendPurchaseEmail>[1]) =>
            sendPurchaseEmail(id, request),
        retry: false,
    })
}

export function usePurchaseEmails(id: string) {
    return useQuery({
        queryKey: ["purchase-orders", id, "emails"],
        queryFn: () => fetchPurchaseEmails(id),
        staleTime: 0,
    })
}
