"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    downloadDepositBankReceipt,
    downloadDepositResolutionEvidence,
    executeDepositCommand,
    fetchDepositAccount,
    fetchDepositAccounts,
    previewDepositBankReceipt,
} from "../api"

import type { DepositAccountListFilters } from "../types"

export const depositKeys = {
    all: ["supplier-deposits"] as const,
    list: (filters: DepositAccountListFilters) =>
        [...depositKeys.all, "list", filters] as const,
    detail: (id: string) => [...depositKeys.all, "detail", id] as const,
}
export function useDepositAccounts(
    filters: DepositAccountListFilters,
    enabled: boolean,
) {
    return useQuery({
        queryKey: depositKeys.list(filters),
        queryFn: () => fetchDepositAccounts(filters),
        enabled,
    })
}
export function useDepositAccount(id: string, enabled: boolean) {
    return useQuery({
        queryKey: depositKeys.detail(id),
        queryFn: () => fetchDepositAccount(id),
        enabled,
    })
}
export function useDepositCommand() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: executeDepositCommand,
        retry: false,
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: depositKeys.all })
        },
    })
}

export function useDepositResolutionEvidence(assetId: string) {
    return useMutation({
        mutationFn: () => downloadDepositResolutionEvidence(assetId),
        retry: false,
    })
}

export function useDepositBankReceipt(assetId: string) {
    const download = useMutation({
        mutationFn: () => downloadDepositBankReceipt(assetId),
        retry: false,
    })
    const preview = useMutation({
        mutationFn: async () => {
            const blob = await previewDepositBankReceipt(assetId)
            const url = URL.createObjectURL(blob)
            window.open(url, "_blank", "noopener,noreferrer")
            window.setTimeout(() => URL.revokeObjectURL(url), 60_000)
        },
        retry: false,
    })
    return { download, preview }
}
