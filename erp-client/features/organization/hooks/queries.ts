"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    createDataScope,
    deleteDataScope,
    fetchDataScopes,
} from "@/features/organization/api/data-scopes"
import {
    fetchOrganizationState,
    previewOrganizationChange,
    submitOrganizationChange,
} from "@/features/organization/api/org-units"
import type {
    CreateDataScopeInput,
    DataScopeUrlState,
    OrganizationChangeRequest,
} from "@/features/organization/types"

export const organizationKeys = {
    all: ["organization"] as const,
    state: () => [...organizationKeys.all, "state"] as const,
}

export const dataScopeKeys = {
    all: ["data-scopes"] as const,
    list: (query: DataScopeUrlState) =>
        [...dataScopeKeys.all, "list", query] as const,
}

export function useOrganizationStateQuery(enabled = true) {
    return useQuery({
        queryKey: organizationKeys.state(),
        queryFn: fetchOrganizationState,
        enabled,
    })
}

export function useDataScopesQuery(url: DataScopeUrlState, enabled = true) {
    return useQuery({
        queryKey: dataScopeKeys.list(url),
        queryFn: () => fetchDataScopes(url),
        enabled,
    })
}

export function usePreviewOrganizationChangeMutation() {
    return useMutation({
        mutationFn: (request: OrganizationChangeRequest) =>
            previewOrganizationChange(request),
    })
}

export function useSubmitOrganizationChangeMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (request: OrganizationChangeRequest) =>
            submitOrganizationChange(request),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: organizationKeys.all,
            })
        },
    })
}

export function useCreateDataScopeMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (input: CreateDataScopeInput) => createDataScope(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: dataScopeKeys.all })
        },
    })
}

export function useDeleteDataScopeMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (id: string) => deleteDataScope(id),
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: dataScopeKeys.all })
        },
    })
}
