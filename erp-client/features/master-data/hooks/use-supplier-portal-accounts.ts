"use client"

import { useQuery } from "@tanstack/react-query"
import { listPortalAccounts } from "@/features/master-data/api/supplier-portal-accounts"
import { portalAdminKeys } from "@/features/supplier-portal/hooks/admin"
export { usePortalAdminCommand as usePortalAccountCommand } from "@/features/supplier-portal/hooks/admin"

export const usePortalAccounts = (
    query: Record<string, unknown>,
    enabled: boolean,
) =>
    useQuery({
        queryKey: [...portalAdminKeys.all, "accounts", query],
        queryFn: () => listPortalAccounts(query),
        enabled,
    })
