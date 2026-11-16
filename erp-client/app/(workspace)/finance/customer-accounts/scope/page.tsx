import type { Metadata } from "next"
import { Suspense } from "react"

import { ScopePagePulse } from "@/components/layout/page-pulse"
import { ReceivableScopePage } from "@/features/customer-receivables/pages/receivable-scope-page"

export const metadata: Metadata = {
    title: "客户往来汇总",
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 恢复
 * view / q / salesOwnerUserIds / operatorUserIds / operatorKind /
 * orgUnitIds / includeDescendants / page / scopeVersion。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<ScopePagePulse />}>
            <ReceivableScopePage />
        </Suspense>
    )
}
