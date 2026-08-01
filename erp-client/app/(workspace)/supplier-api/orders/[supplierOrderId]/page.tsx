import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierOrderCenterPage } from "@/features/supplier-orders/supplier-order-center-page"

export const metadata: Metadata = {
  title: "供应商订单中心",
}

/**
 * 对象中心 TaskTab 身份：supplier-fulfillment-order:{supplierOrderId}
 * 业务数据与动作在客户端 TanStack Query 完成。
 */
export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ supplierOrderId: string }>
  searchParams: Promise<{ section?: string }>
}) {
  const { supplierOrderId } = await params
  const { section } = await searchParams
  return (
    <Suspense
      fallback={
        <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
          <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
          <div className="h-40 animate-pulse rounded-2xl bg-muted" />
        </div>
      }
    >
      <SupplierOrderCenterPage
        key={`${supplierOrderId}-${section ?? "overview"}`}
        supplierOrderId={supplierOrderId}
        section={section}
      />
    </Suspense>
  )
}
