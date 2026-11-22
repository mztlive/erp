import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { SupplierDetailPage } from "@/features/master-data/pages/supplier-detail-page"

export const metadata: Metadata = {
    title: "供应商详情",
}

export default async function Page({
    params,
}: {
    params: Promise<{ stableId: string }>
}) {
    const { stableId } = await params
    return (
        <Suspense fallback={<DetailWorkspaceSkeleton title="供应商详情" />}>
            <SupplierDetailPage key={stableId} stableId={stableId} />
        </Suspense>
    )
}
