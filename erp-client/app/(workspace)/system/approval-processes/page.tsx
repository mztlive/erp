import type { Metadata } from "next"
import { Suspense } from "react"

import { ApprovalProcessesPage } from "@/features/approval-processes/pages/approval-processes-page"

export const metadata: Metadata = {
    title: "审批流程配置",
}

function ApprovalProcessesFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-20 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/**
 * SPA 壳：目录筛选由客户端 useSearchParams 恢复。
 * 业务数据不在服务端 fetch。
 */
export default function ApprovalProcessesRoutePage() {
    return (
        <Suspense fallback={<ApprovalProcessesFallback />}>
            <ApprovalProcessesPage />
        </Suspense>
    )
}
