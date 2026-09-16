import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { SupplierScopePage } from "@/features/supplier-payables/pages/supplier-scope-page"

export const metadata: Metadata = {
    title: "供应商往来（按数据范围）",
}

function ScopeFallback() {
    return (
        <PageScaffold>
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-12 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </PageScaffold>
    )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 恢复
 * view / q / procurementOwnerUserIds / operatorUserIds /
 * orgUnitIds / includeDescendants / page / scopeVersion。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<ScopeFallback />}>
            <SupplierScopePage />
        </Suspense>
    )
}
