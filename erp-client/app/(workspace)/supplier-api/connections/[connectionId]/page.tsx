import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierApiConnectionDetailPage } from "@/features/supplier-api-connections/pages/supplier-api-connection-detail-page"

export const metadata: Metadata = {
    title: "供应商连接详情",
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
 * 独立连接详情页；路径决定连接身份，section 只切换当前页签内的分区。
 */
export default function Page() {
    return (
        <Suspense fallback={<CenterFallback />}>
            <SupplierApiConnectionDetailPage />
        </Suspense>
    )
}
