import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { BackgroundJobsPage } from "@/features/background-jobs/pages/background-jobs-page"

export const metadata: Metadata = {
    title: "批量任务记录",
}

function BackgroundJobsFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-48 animate-pulse rounded-lg bg-muted",
                "h-24 animate-pulse rounded-lg bg-muted",
                "h-64 animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

/**
 * SPA 壳：列表筛选只在客户端维护，不做服务端取数。
 */
export default function Page() {
    return (
        <Suspense fallback={<BackgroundJobsFallback />}>
            <BackgroundJobsPage />
        </Suspense>
    )
}
