"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    createContractExportJob,
    fetchContractCenter,
    fetchContracts,
    uploadContractPdf,
    setContractSalesAccess,
} from "@/features/contracts/api/contracts"
import type { UploadContractPdfInput } from "@/features/contracts/types"
import { useScopedListQuery } from "@/lib/use-scoped-list-query"
import { previewContractFile } from "../api/preview"

const contractKeys = {
    all: ["contracts"] as const,
    list: () => [...contractKeys.all, "list"] as const,
    detail: (id: string) => [...contractKeys.all, "detail", id] as const,
    selectable: () => [...contractKeys.all, "selectable-for-so"] as const,
}

export function useContractsQuery(
    query: import("../lib/contracts-url-state").ContractsUrlState,
) {
    return useScopedListQuery({
        query,
        queryKey: (value) => [...contractKeys.list(), value],
        fetchPage: fetchContracts,
        keepPreviousData: true,
    })
}

export function useContractCenterQuery(contractId: string) {
    return useQuery({
        queryKey: contractKeys.detail(contractId),
        queryFn: () => fetchContractCenter(contractId),
        enabled: Boolean(contractId),
    })
}

export function useContractFilePreviewQuery(
    contractId: string,
    fileId: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [...contractKeys.detail(contractId), "preview", fileId],
        queryFn: () => previewContractFile(contractId, fileId),
        enabled: enabled && Boolean(contractId && fileId),
        staleTime: Infinity,
        gcTime: 0,
        retry: false,
    })
}

export function useUploadContractPdfMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (input: UploadContractPdfInput) => uploadContractPdf(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: contractKeys.list(),
            })
            await queryClient.invalidateQueries({
                queryKey: ["contract-imports"],
            })
            await queryClient.invalidateQueries({
                queryKey: contractKeys.selectable(),
            })
        },
    })
}

export function useCreateContractExportJobMutation() {
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: createContractExportJob,
    })
}

export function useContractSalesAccessMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: setContractSalesAccess,
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: contractKeys.all })
        },
    })
}
