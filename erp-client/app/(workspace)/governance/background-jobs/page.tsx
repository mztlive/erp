import type { Metadata } from "next"
import { Suspense } from "react"

import { BackgroundJobsPage } from "@/features/background-jobs/pages/background-jobs-page"

export const metadata: Metadata = {
    title: "后台任务",
}

function BackgroundJobsFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="h-64 animate-pulse rounded-lg bg-muted" />
        </div>
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
