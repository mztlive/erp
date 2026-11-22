import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { SupplierSettlementDetailPage } from "@/features/supplier-settlements/pages/supplier-settlement-detail-page"

export const metadata: Metadata = {
    title: "结算单详情",
}

/**
 * 独立结算单详情路由 `/supplier-api/settlements/:statementId`。
 * 保留 section / diff / returnTo / workItemId / queueContextId / from 全部参数。
 */
export default async function Page({
    params,
}: {
    params: Promise<{ statementId: string }>
}) {
    const { statementId } = await params
    return (
        <Suspense
            fallback={
                <DetailWorkspaceSkeleton
                    title="供应商结算单"
                    description="正在加载结算单，请稍候…"
                    summary
                />
            }
        >
            <SupplierSettlementDetailPage statementId={statementId} />
        </Suspense>
    )
}
