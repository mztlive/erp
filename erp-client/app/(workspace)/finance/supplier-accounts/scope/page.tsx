import type { Metadata } from "next"
import { Suspense } from "react"

import { ScopePagePulse } from "@/components/layout/page-pulse"
import { SupplierScopePage } from "@/features/supplier-payables/pages/supplier-scope-page"

export const metadata: Metadata = {
    title: "供应商往来汇总",
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 恢复
 * view / q / procurementOwnerUserIds / operatorUserIds /
 * orgUnitIds / includeDescendants / page / scopeVersion。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<ScopePagePulse />}>
            <SupplierScopePage />
        </Suspense>
    )
}
