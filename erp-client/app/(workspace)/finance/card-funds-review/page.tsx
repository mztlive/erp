import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { CardFundsReviewPage } from "@/features/card-funds-review/pages/card-funds-review-page"

export const metadata: Metadata = {
    title: "卡券票款复核",
}

function CardFundsReviewFallback() {
    return (
        <PageScaffold>
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="grid gap-4 xl:grid-cols-[minmax(0,2fr)_minmax(16rem,1fr)]">
                <div className="h-80 animate-pulse rounded-lg bg-muted" />
                <div className="h-64 animate-pulse rounded-lg bg-muted" />
            </div>
        </PageScaffold>
    )
}

/**
 * SPA 壳：URL 由客户端恢复 type / scope / status / currentWorkItemId / queueContextId / autoNext。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<CardFundsReviewFallback />}>
            <CardFundsReviewPage />
        </Suspense>
    )
}
