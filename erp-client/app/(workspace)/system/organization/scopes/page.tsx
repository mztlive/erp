import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { DataScopesPage } from "@/features/organization/pages/data-scopes-page"

export const metadata: Metadata = {
    title: "范围配置",
}

function DataScopesFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-48 animate-pulse rounded-lg bg-muted",
                "h-[28rem] animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

/** SPA 壳：范围配置按资源与动作在客户端查询。 */
export default function SystemOrganizationScopesRoutePage() {
    return (
        <Suspense fallback={<DataScopesFallback />}>
            <DataScopesPage />
        </Suspense>
    )
}
