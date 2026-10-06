"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import * as api from "./api"
export const portalAdminKeys = { all: ["supplier-portal-admin"] as const }
export function usePortalAdminAccess() {
    const profile = useAccountProfileQuery()
    return {
        profile,
        can: (permission: string) =>
            hasPermission(profile.data?.permissions, permission),
    }
}
export const usePortalAccounts = (
    query: Record<string, unknown>,
    enabled: boolean,
) =>
    useQuery({
        queryKey: [...portalAdminKeys.all, "accounts", query],
        queryFn: () => api.listPortalAccounts(query),
        enabled,
    })
export const usePortalGrants = (
    query: Record<string, unknown>,
    enabled: boolean,
) =>
    useQuery({
        queryKey: [...portalAdminKeys.all, "grants", query],
        queryFn: () => api.listPortalGrants(query),
        enabled,
    })
export const usePortalAdminApplications = (
    query: Record<string, unknown>,
    enabled: boolean,
) =>
    useQuery({
        queryKey: [...portalAdminKeys.all, "applications", query],
        queryFn: () => api.listPortalApplications(query),
        enabled,
    })
export const usePortalAdminApplication = (id: string, enabled: boolean) =>
    useQuery({
        queryKey: [...portalAdminKeys.all, "application", id],
        queryFn: () => api.getPortalApplication(id),
        enabled: enabled && !!id,
    })
export const useReviewDictionary = (kind: string, enabled: boolean) =>
    useQuery({
        queryKey: [...portalAdminKeys.all, "dictionary", kind],
        queryFn: () => api.portalReviewDictionaries(kind),
        enabled,
    })
export const usePortalDuplicates = (id: string, q: string, enabled: boolean) =>
    useQuery({
        queryKey: [...portalAdminKeys.all, "duplicates", id, q],
        queryFn: () => api.portalDuplicates(id, q),
        enabled,
    })
export function usePortalAdminCommand<TInput, TResult>(
    execute: (input: TInput) => Promise<TResult>,
) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: execute,
        retry: false,
        meta: { affectsDataScope: true },
        onSuccess: async () => {
            await Promise.all([
                client.invalidateQueries({ queryKey: portalAdminKeys.all }),
                client.invalidateQueries({ queryKey: ["work-items"] }),
                client.invalidateQueries({ queryKey: ["workspace"] }),
                client.invalidateQueries({ queryKey: ["master-data"] }),
            ])
        },
    })
}

export const usePortalOfferingImpacts = (
    offeringId: string,
    params: { page: number; page_size: number },
    enabled: boolean,
) =>
    useQuery({
        queryKey: [
            ...portalAdminKeys.all,
            "offering-impacts",
            offeringId,
            params,
        ],
        queryFn: () => api.getPortalOfferingImpacts(offeringId, params),
        enabled: enabled && !!offeringId,
        retry: false,
    })
export const usePortalReviewFile = (applicationId: string, fileId: string) =>
    useQuery({
        queryKey: [...portalAdminKeys.all, "image", applicationId, fileId],
        queryFn: () => api.portalReviewFile(applicationId, fileId),
        retry: false,
    })
