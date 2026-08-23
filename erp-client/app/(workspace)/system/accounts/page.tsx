import type { Metadata } from "next"
import { Suspense } from "react"

import { AccountsPage } from "@/features/admin/pages/accounts-page"

export const metadata: Metadata = {
    title: "账号管理",
}

function AccountsFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </div>
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
