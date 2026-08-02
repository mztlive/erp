import type { Metadata } from "next"
import { Suspense } from "react"

import { ExternalProductSupplyPage } from "@/features/external-product-supply/external-product-supply-page"

export const metadata: Metadata = {
  title: "商品供给管理",
}

function CatalogFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
      <div className="h-16 animate-pulse rounded-2xl bg-muted" />
      <div className="grid gap-4 xl:grid-cols-[minmax(0,58fr)_minmax(16rem,42fr)]">
        <div className="h-80 animate-pulse rounded-2xl bg-muted" />
        <div className="h-80 animate-pulse rounded-2xl bg-muted" />
      </div>
    </div>
  )
}

/**
 * SPA 壳：URL 恢复 changeType / currentExternalProductId / currentWorkItemId /
 * queueContextId / autoNext / demoRole。业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<CatalogFallback />}>
      <ExternalProductSupplyPage />
    </Suspense>
  )
}
