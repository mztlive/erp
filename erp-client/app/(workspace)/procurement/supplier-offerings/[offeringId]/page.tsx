import type { Metadata } from "next"
import { Suspense } from "react"
import { SupplierOfferingDetailPage } from "@/features/supplier-offerings/pages/supplier-offering-detail-page"

export const metadata: Metadata = { title: "供给资料" }

export default async function Page({
    params,
}: {
    params: Promise<{ offeringId: string }>
}) {
    const { offeringId } = await params
    return (
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载供给资料…
                </div>
            }
        >
            <SupplierOfferingDetailPage
                key={offeringId}
                offeringId={offeringId}
            />
        </Suspense>
    )
}
