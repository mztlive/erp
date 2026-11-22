import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { ProposalDetailPage } from "@/features/sales-selection/pages/proposal-detail-page"

export const metadata: Metadata = {
    title: "销售方案",
}

/**
 * SPA 壳：销售方案详情由客户端取数，只读展示。
 */
export default async function SalesSelectionProposalPage({
    params,
    searchParams,
}: {
    params: Promise<{ proposalId: string }>
    searchParams: Promise<{ returnTo?: string }>
}) {
    const { proposalId } = await params
    const { returnTo } = await searchParams
    return (
        <Suspense
            fallback={
                <DetailWorkspaceSkeleton
                    title="销售方案"
                    description="正在加载销售方案…"
                    sidebar
                />
            }
        >
            <ProposalDetailPage
                key={proposalId}
                proposalId={proposalId}
                returnTo={returnTo}
            />
        </Suspense>
    )
}
