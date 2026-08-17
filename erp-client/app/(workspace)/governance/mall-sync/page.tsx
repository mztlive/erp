import type { Metadata } from "next"
import { Suspense } from "react"

import { MallSyncPage } from "@/features/mall-sync/pages/mall-sync-page"

export const metadata: Metadata = {
    title: "商城同步与映射",
}

function MallSyncFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
            <div className="h-16 animate-pulse rounded-xl bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="grid gap-4 lg:grid-cols-2">
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </div>
        </div>
    )
}

/**
 * SPA 壳：URL 由客户端恢复 view / jobId / snapshotId / mappingTaskId /
 * workItemId / queueContextId 等。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<MallSyncFallback />}>
            <MallSyncPage />
        </Suspense>
    )
}
