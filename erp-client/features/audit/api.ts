import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type { AuditLogItem, AuditLogListParams } from "./types"

export function fetchAuditLogs(params: AuditLogListParams) {
    return apiGet<Page<AuditLogItem>>("/admin/audit-logs", params)
}
