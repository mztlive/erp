import type { Metadata } from "next"
import { Suspense } from "react"

import { ConsumptionOrdersListPage } from "@/features/mall-consumption-orders/consumption-orders-list-page"

export const metadata: Metadata = {
  title: "商城消费订单",
}

function ListFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
      <div className="h-16 animate-pulse rounded-xl bg-muted" />
      <div className="h-24 animate-pulse rounded-2xl bg-muted" />
      <div className="h-72 animate-pulse rounded-2xl bg-muted" />
    </div>
  )
}

/**
 * SPA 壳：URL 恢复 q / mall / fulfillmentChain / attributionStatus / metric / page。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<ListFallback />}>
      <ConsumptionOrdersListPage />
    </Suspense>
  )
}
