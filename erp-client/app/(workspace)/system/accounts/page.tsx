import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { AccountsPage } from "@/features/admin/pages/accounts-page"

export const metadata: Metadata = {
    title: "账号管理",
}

function AccountsFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-48 animate-pulse rounded-lg bg-muted",
                "h-[28rem] animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

/** SPA 壳：账号列表与表单全部在客户端执行。 */
export default function SystemAccountsRoutePage() {
    return (
        <Suspense fallback={<AccountsFallback />}>
            <AccountsPage />
        </Suspense>
    )
}
