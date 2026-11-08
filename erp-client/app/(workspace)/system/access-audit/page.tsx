import { PagePulseShell } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { AccessAuditPage } from "@/features/access-audit/pages/access-audit-page"

export const metadata: Metadata = {
    title: "权限与审计",
}

function AccessAuditFallback() {
    return (
        <PagePulseShell>
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-12 animate-pulse rounded-xl bg-muted" />
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                {Array.from({ length: 4 }).map((_, i) => (
                    <div
                        key={i}
                        className="h-20 animate-pulse rounded-lg bg-muted"
                    />
                ))}
            </div>
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </PagePulseShell>
    )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 恢复
 * view=roles|audit 与 subjectId / eventId 等（view=users 旧链接回退到 roles）。
 * 业务数据不在服务端 fetch。
 */
export default function AccessAuditRoutePage() {
    return (
        <Suspense fallback={<AccessAuditFallback />}>
            <AccessAuditPage />
        </Suspense>
    )
}
