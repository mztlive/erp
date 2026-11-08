import type { Metadata } from "next"
import { Suspense } from "react"

import { RouteLoading } from "@/components/layout/route-loading"
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
        <Suspense fallback={<RouteLoading label="正在加载仓库资料…" />}>
            <WarehouseObjectPage
                key={`${stableId}-${section ?? "overview"}`}
                stableId={stableId}
                section={section}
            />
        </Suspense>
    )
}
