"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    createContractExportJob,
    fetchContractCenter,
    fetchContracts,
    uploadContractPdf,
} from "@/features/contracts/api/contracts"
import type { UploadContractPdfInput } from "@/features/contracts/types"

const contractKeys = {
    all: ["contracts"] as const,
    list: () => [...contractKeys.all, "list"] as const,
    detail: (id: string) => [...contractKeys.all, "detail", id] as const,
    selectable: () => [...contractKeys.all, "selectable-for-so"] as const,
}

export function useContractsQuery(
    query: import("../lib/contracts-url-state").ContractsUrlState,
) {
    const queryClient = useQueryClient()
    const firstPage = { ...query, page: 1, scopeVersion: undefined }
    const baseline = queryClient.getQueryData<
        import("../api/list").ContractListData
    >([...contractKeys.list(), firstPage])
    const scopeVersion =
        query.scopeVersion ??
        (query.page > 1 ? baseline?.scopeVersion : undefined)
    const scoped = { ...query, scopeVersion }
    return useQuery({
        queryKey: [...contractKeys.list(), scoped],
        queryFn: async () => {
            if (query.page === 1 || scopeVersion) return fetchContracts(scoped)
            const first = await queryClient.fetchQuery({
                queryKey: [...contractKeys.list(), firstPage],
                queryFn: () => fetchContracts(firstPage),
            })
            return fetchContracts({
                ...query,
                scopeVersion: first.scopeVersion,
            })
        },
        placeholderData: (previous) => previous,
    })
}

export function useContractCenterQuery(contractId: string) {
    return useQuery({
        queryKey: contractKeys.detail(contractId),
        queryFn: () => fetchContractCenter(contractId),
        enabled: Boolean(contractId),
    })
}

export function useUploadContractPdfMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (input: UploadContractPdfInput) => uploadContractPdf(input),
        onSuccess: async (data) => {
            await queryClient.invalidateQueries({
                queryKey: contractKeys.list(),
            })
            await queryClient.invalidateQueries({
                queryKey: contractKeys.detail(data.contractId),
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
