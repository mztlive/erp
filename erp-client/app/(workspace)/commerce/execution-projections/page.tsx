import type { Metadata } from "next"
import { Suspense } from "react"

import { ExecutionProjectionsPage } from "@/features/execution-projections/execution-projections-page"

export const metadata: Metadata = {
  title: "执行投影",
}

function ExecutionProjectionsFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
      <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
      <div className="h-16 animate-pulse rounded-xl bg-muted" />
      <div className="h-24 animate-pulse rounded-2xl bg-muted" />
      <div className="h-72 animate-pulse rounded-2xl bg-muted" />
    </div>
  )
}

/**
 * SPA 壳：URL 恢复 q / mall / deliveryStatus / latency / reconciliation /
 * metric / projectionId / revision / page。业务数据仅在客户端 Query 获取。
 */
export default function Page() {
  return (
    <Suspense fallback={<ExecutionProjectionsFallback />}>
      <ExecutionProjectionsPage />
    </Suspense>
  )
}
