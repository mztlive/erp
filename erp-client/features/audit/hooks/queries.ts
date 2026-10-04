"use client"

import { useQuery } from "@tanstack/react-query"
import { fetchAuditLogs } from "../api"
import type { AuditLogListParams } from "../types"

const auditLogKeys = {
    all: ["business-audit"] as const,
    list: (params: AuditLogListParams) =>
        [...auditLogKeys.all, "list", params] as const,
}

export function useAuditLogsQuery(params: AuditLogListParams) {
    return useQuery({
        queryKey: auditLogKeys.list(params),
        queryFn: () => fetchAuditLogs(params),
    })
}
