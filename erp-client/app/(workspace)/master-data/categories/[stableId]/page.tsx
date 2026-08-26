import type { Metadata } from "next"
import { Suspense } from "react"

import { CategoryObjectPage } from "@/features/master-data/pages/category-object-page"

export const metadata: Metadata = {
    title: "商品分类详情",
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
                    正在加载商品分类…
                </div>
            }
        >
            <CategoryObjectPage
                key={`${stableId}-${section ?? "overview"}`}
                stableId={stableId}
                section={section}
            />
        </Suspense>
    )
}
