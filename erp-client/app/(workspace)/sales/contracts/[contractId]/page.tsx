import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
import { ContractDetailPage } from "@/features/contracts/pages/contract-detail-page"

export const metadata: Metadata = {
    title: "合同详情",
}

export default async function Page({
    params,
    searchParams,
}: {
    params: Promise<{ contractId: string }>
    searchParams: Promise<{ section?: string }>
}) {
    const { contractId } = await params
    const { section } = await searchParams
    return (
        <Suspense fallback={<RouteLoading label="正在加载合同…" />}>
            <ContractDetailPage
                key={contractId}
                contractId={contractId}
                section={section}
            />
        </Suspense>
    )
}
