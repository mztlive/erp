import type { Metadata } from "next"
import { Suspense } from "react"

import { PurchaseOrderDetailPage } from "@/features/purchase-orders/purchase-order-detail-page"

export const metadata: Metadata = {
  title: "采购单对象中心",
}

export default async function PurchaseOrderObjectPage({
  params,
  searchParams,
}: {
  params: Promise<{ purchaseOrderId: string }>
  searchParams: Promise<{ section?: string; mode?: string }>
}) {
  const { purchaseOrderId } = await params
  const { section, mode } = await searchParams
  return (
    <Suspense
      fallback={
        <div className="p-5 text-sm text-muted-foreground">
          正在加载采购单…
        </div>
      }
    >
      <PurchaseOrderDetailPage
        key={`${purchaseOrderId}-${section ?? "overview"}-${mode ?? "view"}`}
        purchaseOrderId={purchaseOrderId}
        section={section}
        mode={mode}
      />
    </Suspense>
  )
}
