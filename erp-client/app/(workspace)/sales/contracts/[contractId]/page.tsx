import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { ContractDetailPage } from "@/features/contracts/pages/contract-detail-page"
import { resolveSection } from "@/features/contracts/lib/contract-detail-helpers"

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
        <Suspense
            fallback={
                <DetailWorkspaceSkeleton
                    title="合同"
                    description="正在加载合同…"
                    sidebar={resolveSection(section) === "overview"}
                />
            }
        >
            <ContractDetailPage
                key={contractId}
                contractId={contractId}
                section={section}
            />
        </Suspense>
    )
}
