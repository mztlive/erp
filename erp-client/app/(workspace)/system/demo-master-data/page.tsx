import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { DemoMasterDataPage } from "@/features/demo-master-data/pages/demo-master-data-page"

export const metadata: Metadata = {
    title: "演示主数据",
}

function DemoMasterDataFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-48 animate-pulse rounded-lg bg-muted",
                "h-40 animate-pulse rounded-lg bg-muted",
            ]}
        />
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
