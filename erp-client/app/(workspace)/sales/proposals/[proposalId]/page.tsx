import type { Metadata } from "next"

import { ProposalDetailPage } from "@/features/sales-selection/pages/proposal-detail-page"

export const metadata: Metadata = {
    title: "销售方案",
}

export default async function SalesProposalPage({
    params,
}: {
    params: Promise<{ proposalId: string }>
}) {
    const { proposalId } = await params
    return <ProposalDetailPage proposalId={proposalId} />
}
