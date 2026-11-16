import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierApiConnectionsPage } from "@/features/supplier-api-connections/pages/supplier-api-connections-page"

export const metadata: Metadata = {
    title: "供应商平台连接",
}

function ConnectionsFallback() {
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
 * 列表 SPA 壳：URL 恢复筛选和分页；旧查询参数详情地址跳转到独立路由。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<ConnectionsFallback />}>
            <SupplierApiConnectionsPage />
        </Suspense>
    )
}
