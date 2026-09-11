import type { Metadata } from "next"
import { Suspense } from "react"

import { ProposalDetailPage } from "@/features/sales-selection/pages/proposal-detail-page"

export const metadata: Metadata = {
    title: "销售方案",
}

/**
 * SPA 壳：销售方案详情由客户端取数，只读展示。
 */
export default async function SalesSelectionProposalPage({
    params,
}: {
    params: Promise<{ proposalId: string }>
}) {
    const { proposalId } = await params
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载销售方案…
                </div>
            }
        >
            <ProposalDetailPage key={proposalId} proposalId={proposalId} />
        </Suspense>
    )
}
