import type { Metadata } from "next"
import { Suspense } from "react"

import { WarehouseObjectPage } from "@/features/master-data/components/warehouse/warehouse-object-page"

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
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载仓库资料…
                </div>
            }
        >
            <WarehouseObjectPage
                key={`${stableId}-${section ?? "overview"}`}
                stableId={stableId}
                section={section}
            />
        </Suspense>
    )
}
