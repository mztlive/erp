"use client"

import { useMutation, useQuery } from "@tanstack/react-query"
import {
    fetchTransferPreview,
    verifyTransferPreview,
} from "../api/transfer-preview"

export function useTransferPreviewQuery<T>(jobId: string, enabled: boolean) {
    return useQuery({
        queryKey: ["sales-inquiries", "transfer-preview", jobId],
        queryFn: () => fetchTransferPreview<T>(jobId),
        enabled,
        staleTime: 0,
        gcTime: 0,
        retry: false,
        refetchOnWindowFocus: "always",
    })
}

export function useVerifyTransferPreviewMutation(jobId: string) {
    return useMutation({
        mutationFn: () => verifyTransferPreview(jobId),
        meta: { suppressErrorToast: true },
    })
}
