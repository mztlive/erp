import type { Metadata } from "next"
import { Suspense } from "react"

import { HistoryBackfillPage } from "@/features/history-backfill/history-backfill-page"

export const metadata: Metadata = {
    title: "历史消费回填",
}

function HistoryBackfillFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
            <div className="h-16 animate-pulse rounded-xl bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="h-72 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/**
 * SPA 壳：URL 由客户端恢复 view / processingStatus / reportReviewStatus / mall 等。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<HistoryBackfillFallback />}>
            <HistoryBackfillPage />
        </Suspense>
    )
}
