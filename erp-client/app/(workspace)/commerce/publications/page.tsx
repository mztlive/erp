import type { Metadata } from "next"
import { Suspense } from "react"

import { ProductPublicationsListPage } from "@/features/product-publications/pages/product-publications-list-page"

export const metadata: Metadata = {
    title: "商品发布",
}

function PublicationsFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-16 animate-pulse rounded-xl bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="h-72 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

/**
 * SPA 壳：URL 由客户端恢复 q / mall / publicationStatus / deliveryStatus / metric。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
    return (
        <Suspense fallback={<PublicationsFallback />}>
            <ProductPublicationsListPage />
        </Suspense>
    )
}
