import type { Metadata } from "next"
import { Suspense } from "react"

import { ConsumptionOrderCenterPage } from "@/features/mall-consumption-orders/consumption-order-center-page"

export const metadata: Metadata = {
  title: "商城消费订单",
}

function CenterFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
      <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
      <div className="h-24 animate-pulse rounded-xl bg-muted" />
      <div className="h-40 animate-pulse rounded-lg bg-muted" />
      <div className="grid gap-4 xl:grid-cols-[1fr_20rem]">
        <div className="h-96 animate-pulse rounded-lg bg-muted" />
        <div className="h-48 animate-pulse rounded-lg bg-muted" />
      </div>
    </div>
  )
}

/**
 * SPA 壳：mallOrderId 为稳定页签身份；section / fact 由客户端 URL 恢复。
 * 业务数据不在服务端 fetch。
 */
export default async function Page({
  params,
}: {
  params: Promise<{ mallOrderId: string }>
}) {
  const { mallOrderId } = await params
  return (
    <Suspense fallback={<CenterFallback />}>
      <ConsumptionOrderCenterPage mallOrderId={mallOrderId} />
    </Suspense>
  )
}
