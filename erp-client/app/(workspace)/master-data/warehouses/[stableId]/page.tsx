import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { WarehouseObjectPage } from "@/features/master-data/pages/warehouse-object-page"

export const metadata: Metadata = {
    title: "仓库详情",
}

export default async function Page({
    params,
    searchParams,
}: {
    params: Promise<{ stableId: string }>
    searchParams: Promise<{ section?: string }>
}) {
    const { stableId } = await params
    const { section } = await searchParams
    return (
        <Suspense fallback={<DetailWorkspaceSkeleton title="仓库详情" />}>
            <WarehouseObjectPage
                key={stableId}
                stableId={stableId}
                section={section}
            />
        </Suspense>
    )
}
