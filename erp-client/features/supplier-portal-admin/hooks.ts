"use client"
import { useQuery } from "@tanstack/react-query"
import {
    portalAdminKeys,
    usePortalAdminAccess,
} from "@/features/supplier-portal/hooks/admin"
export {
    portalAdminKeys,
    usePortalAdminAccess,
    usePortalAdminApplications,
    usePortalAdminCommand,
} from "@/features/supplier-portal/hooks/admin"
import * as api from "./api"
export const portalAdminApplicationKey = (id: string) =>
    [...portalAdminKeys.all, "application", id] as const

export const usePortalAdminApplication = (id: string, enabled: boolean) =>
    useQuery({
        queryKey: portalAdminApplicationKey(id),
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

export function usePortalReviewApplication(applicationId: string) {
    const access = usePortalAdminAccess()
    const canRead = access.can("supplier_portal_request:detail")
    const query = usePortalAdminApplication(applicationId, canRead)
    return {
        access,
        canRead,
        query,
        application: canRead ? query.data : undefined,
    }
}
