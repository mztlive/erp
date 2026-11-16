import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { ApprovalProcessDetailPage } from "@/features/approval-processes/pages/approval-process-detail-page"

export const metadata: Metadata = {
    title: "审批流程设置",
}

function ApprovalProcessDetailFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-56 animate-pulse rounded-lg bg-muted",
                "h-[28rem] animate-pulse rounded-lg bg-muted",
            ]}
        />
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
