import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
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
                <PageScaffold>
                    <div className="h-12 w-48 animate-pulse rounded-lg bg-muted" />
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </PageScaffold>
            }
        >
            <InvoiceRequestDetailPage requestId={requestId} />
        </Suspense>
    )
}
