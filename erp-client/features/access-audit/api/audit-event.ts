// 审计事件单条读取：追加式事件只读。

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type { AuditEventRow } from "@/features/access-audit/types"
import type { BackendAuditEvent } from "./backend-types"
import { toAuditRow } from "./mappers"

export async function fetchAuditEvent(
    eventId: string,
): Promise<AuditEventRow | null> {
    const page = await apiGet<Page<BackendAuditEvent>>("/admin/audit-events", {
        page: 1,
        page_size: 100,
    })
    const hit = page.items.find((e) => e.id === eventId)
    return hit ? toAuditRow(hit) : null
}
