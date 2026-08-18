import type { Metadata } from "next"
import { Suspense } from "react"

import { ApprovalProcessDetailPage } from "@/features/approval-processes/pages/approval-process-detail-page"

export const metadata: Metadata = {
    title: "审批流程定义",
}

function ApprovalProcessDetailFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/**
 * SPA 壳：定义详情与草稿编辑在客户端取数。
 */
export default async function ApprovalProcessDetailRoutePage({
    params,
}: {
    params: Promise<{ documentType: string }>
}) {
    const { documentType } = await params
    return (
        <Suspense fallback={<ApprovalProcessDetailFallback />}>
            <ApprovalProcessDetailPage documentType={documentType} />
        </Suspense>
    )
}
