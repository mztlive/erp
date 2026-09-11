"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import * as api from "./api"
import type {
    PoolFilterSnapshot,
    PoolSourceKind,
    SelectionForm,
    SubmitMode,
} from "./types"

export const salesSelectionKeys = {
    all: ["sales-selection"] as const,
    list: (params: unknown) =>
        [...salesSelectionKeys.all, "list", params] as const,
    detail: (id: string) => [...salesSelectionKeys.all, "detail", id] as const,
    proposal: (id: string) =>
        [...salesSelectionKeys.all, "proposal", id] as const,
    public: (token: string) =>
        [...salesSelectionKeys.all, "public", token] as const,
}

export function useBookletsQuery(params: {
    customerId?: string
    form?: SelectionForm
    status?: string
    submitMode?: SubmitMode
}) {
    return useQuery({
        queryKey: salesSelectionKeys.list(params),
        queryFn: () => api.fetchBooklets(params),
    })
}

export function useBookletQuery(id: string | undefined) {
    return useQuery({
        queryKey: salesSelectionKeys.detail(id ?? ""),
        queryFn: () => api.fetchBooklet(id!),
        enabled: Boolean(id),
        refetchInterval: (query) =>
            query.state.data?.status === "PREPARING" ? 2000 : false,
    })
}

export function useProposalQuery(id: string | undefined) {
    return useQuery({
        queryKey: salesSelectionKeys.proposal(id ?? ""),
        queryFn: () => api.fetchProposal(id!),
        enabled: Boolean(id),
    })
}

export function usePublicSelectionQuery(token: string | undefined) {
    return useQuery({
        queryKey: salesSelectionKeys.public(token ?? ""),
        queryFn: () => api.fetchPublicPage(token!),
        enabled: Boolean(token),
        retry: false,
    })
}

export function useCreateBookletMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: {
            idempotencyKey: string
            customerId: string
            form: SelectionForm
            submitMode: SubmitMode
            poolSourceKind: PoolSourceKind
            poolFilter?: PoolFilterSnapshot
            skuIds?: string[]
            tiers: Parameters<typeof api.createBooklet>[0]["tiers"]
        }) => api.createBooklet(input),
        onSuccess: () => {
            void queryClient.invalidateQueries({
                queryKey: salesSelectionKeys.all,
            })
        },
    })
}

export function usePrepareBookletMutation(id: string) {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: api.prepareBooklet.bind(null, id),
        onSuccess: (data) => {
            queryClient.setQueryData(salesSelectionKeys.detail(id), data)
        },
    })
}

export function usePublishBookletMutation(id: string) {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: api.publishBooklet.bind(null, id),
        onSuccess: (data) => {
            queryClient.setQueryData(salesSelectionKeys.detail(id), data)
        },
    })
}
