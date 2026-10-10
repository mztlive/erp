import { Suspense } from "react"
import type { Metadata } from "next"
import { PaymentRequestsPage } from "@/features/purchase-payment-requests/pages/payment-requests-page"

export const metadata: Metadata = { title: "采购付款申请" }
export default async function Page({
    searchParams,
}: {
    searchParams: Promise<Record<string, string | string[] | undefined>>
}) {
    const params = await searchParams
    const purchaseOrderId =
        typeof params.purchaseOrderId === "string"
            ? params.purchaseOrderId
            : undefined
    return (
        <Suspense>
            <PaymentRequestsPage
                purchaseOrderId={purchaseOrderId}
                create={params.create === "1"}
            />
        </Suspense>
    )
}
