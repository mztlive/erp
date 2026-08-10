import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierSettlementsPage } from "@/features/supplier-settlements/supplier-settlements-page"

export const metadata: Metadata = {
    title: "结算单详情",
}

function CenterFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-40 animate-pulse rounded-lg bg-muted" />
            <div className="h-24 animate-pulse rounded-xl bg-muted" />
            <div className="h-64 animate-pulse rounded-lg bg-muted" />
        </div>
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
