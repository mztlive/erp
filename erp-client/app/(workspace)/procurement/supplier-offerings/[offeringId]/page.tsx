import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { SupplierOfferingDetailPage } from "@/features/supplier-offerings/pages/supplier-offering-detail-page"

export const metadata: Metadata = { title: "供货商品详情" }

export default async function Page({
    params,
}: {
    params: Promise<{ offeringId: string }>
}) {
    const { offeringId } = await params
    return (
        <Suspense
            fallback={
                <DetailWorkspaceSkeleton title="供给资料" summary sidebar />
            }
        >
            <SupplierOfferingDetailPage
                key={offeringId}
                offeringId={offeringId}
            />
        </Suspense>
    )
}
