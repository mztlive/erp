import type { Metadata } from "next"
import { Suspense } from "react"

import { FulfillmentOperationsPage } from "@/features/fulfillment-operations/fulfillment-operations-page"

export const metadata: Metadata = {
  title: "履约作业",
}

function FulfillmentFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
      <div className="h-20 animate-pulse rounded-2xl bg-muted" />
      <div className="grid gap-4 xl:grid-cols-[minmax(16rem,1fr)_minmax(0,2fr)]">
        <div className="h-80 animate-pulse rounded-2xl bg-muted" />
        <div className="h-96 animate-pulse rounded-2xl bg-muted" />
      </div>
    </div>
  )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 读取并恢复
 * type / scope / currentWorkItemId / queueContextId / salesOrderId / purchaseOrderId /
 * warehouseId / returnTo / from / autoNext。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<FulfillmentFallback />}>
      <FulfillmentOperationsPage />
    </Suspense>
  )
}
