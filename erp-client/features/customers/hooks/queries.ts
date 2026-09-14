"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    applyCustomerAssignment,
    createCustomer,
    fetchCustomerCenter,
    fetchCustomerDirectory,
    queryCustomerMutationByIdempotency,
    revealCustomerSensitiveField,
    saveCustomerDetails,
} from "@/features/customers/api/index"
import type {
    CreateCustomerInput,
    CustomerAssignmentChangeInput,
    CustomerDirectoryQuery,
    SaveCustomerDetailsInput,
} from "@/features/customers/types"

export { revealCustomerSensitiveField }

const customerKeys = {
    all: ["customers"] as const,
    directory: (query: CustomerDirectoryQuery) =>
        [...customerKeys.all, "directory", query] as const,
    detail: (customerId: string) =>
        [...customerKeys.all, "detail", customerId] as const,
}

export function useCustomerDirectoryQuery(
    query: CustomerDirectoryQuery,
    options?: { enabled?: boolean },
) {
    const queryClient = useQueryClient()
    const firstPage = { ...query, page: 1, scopeVersion: undefined }
    const baseline = queryClient.getQueryData<
        import("@/features/customers/types").CustomerDirectoryResult
    >(customerKeys.directory(firstPage))
    const scopeVersion =
        query.scopeVersion ??
        (query.page > 1 ? baseline?.scopeVersion : undefined)
    const scoped = { ...query, scopeVersion }
    return useQuery({
        queryKey: customerKeys.directory(scoped),
        queryFn: async () => {
            if (query.page === 1 || scopeVersion)
                return fetchCustomerDirectory(scoped)
            const first = await queryClient.fetchQuery({
                queryKey: customerKeys.directory(firstPage),
                queryFn: () => fetchCustomerDirectory(firstPage),
            })
            return fetchCustomerDirectory({
                ...query,
                scopeVersion: first.scopeVersion,
            })
        },
        // 切换筛选时保留上一批结果渲染，避免整卡闪烁（数据表骨架只出现于首载）。
        placeholderData: (previous) => previous,
        enabled: options?.enabled ?? true,
    })
}

export function useCustomerCenterQuery(customerId: string) {
    return useQuery({
        queryKey: customerKeys.detail(customerId),
        queryFn: () => fetchCustomerCenter(customerId),
        enabled: Boolean(customerId),
    })
}

export function useSaveCustomerDetailsMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (input: SaveCustomerDetailsInput) =>
            saveCustomerDetails(input),
        onSuccess: async (result) => {
            if (result.outcome === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerKeys.all,
                })
            }
        },
    })
}

export function useCreateCustomerMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (input: CreateCustomerInput) => createCustomer(input),
        onSuccess: async (result) => {
            if (result.outcome === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: customerKeys.all,
                })
            }
        },
    })
}

export function useQueryCustomerIdempotencyMutation() {
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (idempotencyKey: string) =>
            queryCustomerMutationByIdempotency(idempotencyKey),
    })
}

/** 提交客户责任归属变更并刷新客户对象中心及目录。 */
export function useApplyCustomerAssignmentMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (input: CustomerAssignmentChangeInput) =>
            applyCustomerAssignment(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: customerKeys.all })
        },
    })
}
