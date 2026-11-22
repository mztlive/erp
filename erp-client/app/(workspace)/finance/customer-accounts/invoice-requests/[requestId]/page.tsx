import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { InvoiceRequestDetailPage } from "@/features/invoice-requests/components/request-detail-page"

export const metadata: Metadata = {
    title: "开票申请详情",
}

export default async function Page({
    params,
}: {
    params: Promise<{ requestId: string }>
}) {
    const { requestId } = await params
    return (
        <Suspense
            fallback={
                <DetailWorkspaceSkeleton
                    title="开票申请详情"
                    description="正在读取申请金额、开票资料与审批进度…"
                    summary
                    sidebar
                />
            }
        >
            <InvoiceRequestDetailPage requestId={requestId} />
        </Suspense>
    )
}
