import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierDetailPage } from "@/features/master-data/components/supplier/supplier-detail-page"

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
        <Suspense
            fallback={
                <div className="p-5 text-sm text-muted-foreground">
                    正在加载供应商资料…
                </div>
            }
        >
            <SupplierDetailPage key={stableId} stableId={stableId} />
        </Suspense>
    )
}
