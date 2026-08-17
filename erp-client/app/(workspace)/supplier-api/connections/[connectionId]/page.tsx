import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierApiConnectionsPage } from "@/features/supplier-api-connections/pages/supplier-api-connections-page"

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
 * 路径 `/supplier-api/connections/:connectionId` 与列表查询参数 connectionId 等价。
 * 页签身份：supplier-connection:{connectionId}，同连接不复制。
 */
export default function Page() {
    return (
        <Suspense fallback={<CenterFallback />}>
            <SupplierApiConnectionsPage />
        </Suspense>
    )
}
