import type { Metadata } from "next"
import { Suspense } from "react"

import { DataScopesPage } from "@/features/organization/pages/data-scopes-page"

export const metadata: Metadata = {
    title: "范围配置",
}

function DataScopesFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </div>
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
