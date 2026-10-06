"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import * as api from "../api"
import type {
    PortalBatchMode,
    PortalBatchPhase,
    PortalBatchRow,
} from "../types"

export const portalKeys = {
    all: ["supplier-portal"] as const,
    session: ["supplier-portal", "session"] as const,
    offerings: (query: Record<string, unknown>) =>
        ["supplier-portal", "offerings", query] as const,
    offering: (id: string) => ["supplier-portal", "offering", id] as const,
    applications: (query: Record<string, unknown>) =>
        ["supplier-portal", "applications", query] as const,
    application: (id: string) =>
        ["supplier-portal", "application", id] as const,
}
export const usePortalSession = (enabled = true) =>
    useQuery({
        queryKey: portalKeys.session,
        queryFn: api.portalSession,
        enabled,
        retry: false,
    })
export const usePortalOfferings = (query: Record<string, unknown>) =>
    useQuery({
        queryKey: portalKeys.offerings(query),
        queryFn: () => api.portalOfferings(query),
    })
export const usePortalOffering = (id: string) =>
    useQuery({
        queryKey: portalKeys.offering(id),
        queryFn: () => api.portalOffering(id),
        enabled: !!id,
    })
export const usePortalHistory = (id: string) =>
    useQuery({
        queryKey: [...portalKeys.offering(id), "history"],
        queryFn: () => api.portalOfferingHistory(id),
        enabled: !!id,
    })
export const usePortalCatalog = (
    query: Record<string, unknown>,
    enabled = true,
) =>
    useQuery({
        queryKey: [...portalKeys.all, "catalog", query],
        queryFn: () => api.portalCatalog(query),
        enabled,
    })
export const usePortalCatalogTarget = (id: string) =>
    useQuery({
        queryKey: [...portalKeys.all, "catalog-target", id],
        queryFn: () => api.portalCatalogTarget(id),
        enabled: false,
        retry: false,
    })
export const usePortalDictionaries = (kind: string) =>
    useQuery({
        queryKey: [...portalKeys.all, "dictionaries", kind],
        queryFn: () => api.portalDictionaries(kind),
    })
export const usePortalApplications = (query: Record<string, unknown>) =>
    useQuery({
        queryKey: portalKeys.applications(query),
        queryFn: () => api.portalApplications(query),
    })
export const usePortalApplication = (id: string) =>
    useQuery({
        queryKey: portalKeys.application(id),
        queryFn: () => api.portalApplication(id),
        enabled: !!id,
    })
export const usePortalCooperation = (enabled = true) =>
    useQuery({
        queryKey: [...portalKeys.all, "cooperation"],
        queryFn: api.portalCooperation,
        enabled,
    })
/** 写操作不自动重试；页面保留原命令以确认未知结果。 */
export function usePortalCommand<TInput, TResult>(
    execute: (input: TInput) => Promise<TResult>,
) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: execute,
        retry: false,
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: portalKeys.all })
        },
    })
}
export const usePortalBatch = (mode: PortalBatchMode) =>
    usePortalCommand(
        (input: {
            rows: PortalBatchRow[]
            validateOnly: boolean
            recoveryOnly?: boolean
            phase?: PortalBatchPhase
        }) =>
            api.portalBatch(
                mode,
                input.rows,
                input.validateOnly,
                input.recoveryOnly,
                input.phase,
            ),
    )

export const usePortalFile = (
    assetId: string | null | undefined,
    source: api.PortalFileSource,
) =>
    useQuery({
        queryKey: [...portalKeys.all, "image", source, assetId],
        queryFn: () => api.portalFile(source, assetId!),
        enabled: !!assetId,
        retry: false,
    })

export const usePortalCategorySuggestion = (
    query: { original_category_path: string; product_kind: string },
    enabled = false,
) =>
    useQuery({
        queryKey: [...portalKeys.all, "category-mapping-suggestion", query],
        queryFn: () => api.portalCategoryMappingSuggestion(query),
        enabled: enabled && !!query.original_category_path.trim(),
        retry: false,
    })
