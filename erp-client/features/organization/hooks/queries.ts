"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchOrganizationState,
    previewOrganizationChange,
    submitOrganizationChange,
} from "@/features/organization/api/org-units"
import type {
    DataScopeUrlState,
    OrganizationChangeRequest,
} from "@/features/organization/types"

export const organizationKeys = {
    all: ["organization"] as const,
    /** 组织筛选只做客户端裁剪，请求键固定为 state，不随 unitId/q/kind/status 变化。 */
    state: () => [...organizationKeys.all, "state"] as const,
}

export const dataScopeKeys = {
    all: ["data-scopes"] as const,
    list: (query: DataScopeUrlState) =>
        [...dataScopeKeys.all, "list", query] as const,
}

export function useOrganizationStateQuery(enabled = true) {
    return useQuery({
        enabled,
        queryKey: organizationKeys.state(),
        queryFn: fetchOrganizationState,
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
            await queryClient.invalidateQueries({ queryKey: ["admin"] })
        },
    })
}
