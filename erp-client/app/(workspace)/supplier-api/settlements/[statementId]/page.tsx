import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierSettlementsPage } from "@/features/supplier-settlements/pages/supplier-settlements-page"

export const metadata: Metadata = {
    title: "结算单详情",
}

function CenterFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-40 animate-pulse rounded-lg bg-muted",
                "h-24 animate-pulse rounded-xl bg-muted",
                "h-64 animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

/**
 * 路径 `/supplier-api/settlements/:statementId` 与列表查询参数 statementId 等价。
 * 页签身份：supplier-settlement:{statementId}，同结算单不复制。
 */
export default function Page() {
    return (
        <Suspense fallback={<CenterFallback />}>
            <SupplierSettlementsPage />
        </Suspense>
    )
}
