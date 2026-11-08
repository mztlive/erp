import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierSettlementsPage } from "@/features/supplier-settlements/pages/supplier-settlements-page"

export const metadata: Metadata = {
    title: "API 供应商结算",
}

function SettlementsFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-56 animate-pulse rounded-lg bg-muted",
                "h-16 animate-pulse rounded-xl bg-muted",
                "h-24 animate-pulse rounded-lg bg-muted",
                "h-72 animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

/**
 * SPA 壳：URL 恢复 view / supplier / period / status / differenceType / preview / statementId / section / role。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<SettlementsFallback />}>
            <SupplierSettlementsPage />
        </Suspense>
    )
}
