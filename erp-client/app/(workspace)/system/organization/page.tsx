import type { Metadata } from "next"
import { Suspense } from "react"

import { OrganizationPage } from "@/features/organization/pages/organization-page"

export const metadata: Metadata = {
    title: "组织架构",
}

function OrganizationFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/** SPA 壳：组织树、成员与管理授权全部在客户端执行。 */
export default function SystemOrganizationRoutePage() {
    return (
        <Suspense fallback={<OrganizationFallback />}>
            <OrganizationPage />
        </Suspense>
    )
}
