import { Suspense } from "react"
import type { Metadata } from "next"
import { DetailWorkspaceSkeleton } from "@/components/business"
import { PaymentRequestDetailPage } from "@/features/purchase-payment-requests/pages/payment-request-detail-page"

export const metadata: Metadata = { title: "采购付款申请" }

export default async function Page({
    params,
}: {
    params: Promise<{ requestId: string }>
}) {
    const { requestId } = await params
    return (
        <Suspense
            fallback={<DetailWorkspaceSkeleton title="采购付款申请" summary />}
        >
            <PaymentRequestDetailPage key={requestId} requestId={requestId} />
        </Suspense>
    )
}
