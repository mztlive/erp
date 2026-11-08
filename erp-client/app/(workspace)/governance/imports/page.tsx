import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { ImportOpeningPage } from "@/features/import-opening/pages/import-opening-page"

export const metadata: Metadata = {
    title: "导入与期初",
}

function ImportOpeningFallback() {
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
 * SPA 壳：URL 由客户端恢复 environment / batchId / section / 问题筛选等。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<ImportOpeningFallback />}>
            <ImportOpeningPage />
        </Suspense>
    )
}
