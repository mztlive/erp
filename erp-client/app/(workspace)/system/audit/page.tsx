import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { AuditPage } from "@/features/access-audit/pages/audit-page"

export const metadata: Metadata = {
    title: "审计查询",
}

function AuditFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-48 animate-pulse rounded-lg bg-muted",
                "h-12 animate-pulse rounded-xl bg-muted",
                "h-[28rem] animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

/**
 * SPA 壳：审计查询的时间范围、操作者、对象等筛选由客户端 useSearchParams 恢复。
 * 业务数据不在服务端 fetch。
 */
export default function SystemAuditRoutePage() {
    return (
        <Suspense fallback={<AuditFallback />}>
            <AuditPage />
        </Suspense>
    )
}
