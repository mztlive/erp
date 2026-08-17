import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { CardBusinessAnalyticsPage } from "@/features/card-business-analytics/pages/card-business-analytics-page"

export const metadata: Metadata = {
    title: "卡券消费台账与经营分析",
}

function CardBusinessFallback() {
    return (
        <PageScaffold>
            <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                {Array.from({ length: 8 }).map((_, i) => (
                    <div
                        key={i}
                        className="h-20 animate-pulse rounded-lg bg-muted"
                    />
                ))}
            </div>
            <div className="h-72 animate-pulse rounded-lg bg-muted" />
        </PageScaffold>
    )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 恢复
 * from/to/dateBasis/customerId/salesOrderId/costBasis 等。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<CardBusinessFallback />}>
            <CardBusinessAnalyticsPage />
        </Suspense>
    )
}
