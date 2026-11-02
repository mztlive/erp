"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import * as api from "../admin-api"
export const portalAdminKeys = { all: ["supplier-portal-admin"] as const }
export function usePortalAdminAccess() {
    const profile = useAccountProfileQuery()
    return {
        profile,
        can: (permission: string) =>
            hasPermission(profile.data?.permissions, permission),
    }
}
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
