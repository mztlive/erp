import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
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
        <Suspense fallback={<DetailWorkspaceSkeleton title="商品分类详情" />}>
            <CategoryObjectPage
                key={stableId}
                stableId={stableId}
                section={section}
            />
        </Suspense>
    )
}
