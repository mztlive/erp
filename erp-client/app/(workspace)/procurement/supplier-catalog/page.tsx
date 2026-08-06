import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierCatalogPage } from "@/features/supplier-catalog/supplier-catalog-page"

export const metadata: Metadata = {
  title: "供应商商品库",
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
 * SPA 壳：URL 恢复 changeType / currentSupplierProductId / currentWorkItemId /
 * queueContextId / autoNext。业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<CatalogFallback />}>
      <SupplierCatalogPage />
    </Suspense>
  )
}
