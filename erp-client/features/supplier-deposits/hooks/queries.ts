"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    executeDepositCommand,
    downloadDepositResolutionEvidence,
    fetchDepositAccount,
    fetchDepositAccounts,
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
