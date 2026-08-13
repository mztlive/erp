import type { Metadata } from "next"
import { Suspense } from "react"

import { MasterDataObjectPage } from "@/features/master-data/components/pages/master-data-center-page"

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
                    正在加载基础资料对象…
                </div>
            }
        >
            <MasterDataObjectPage
                key={`${stableId}-${section ?? "overview"}`}
                resource="warehouses"
                stableId={stableId}
                section={section}
            />
        </Suspense>
    )
}
