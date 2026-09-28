import type { Metadata } from "next"
import { Suspense } from "react"

import { DemoMasterDataPage } from "@/features/demo-master-data/pages/demo-master-data-page"

export const metadata: Metadata = {
    title: "演示主数据",
}

function DemoMasterDataFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-40 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/** SPA 壳：演示主数据的生成和删除在客户端执行。 */
export default function DemoMasterDataRoutePage() {
    return (
        <Suspense fallback={<DemoMasterDataFallback />}>
            <DemoMasterDataPage />
        </Suspense>
    )
}
