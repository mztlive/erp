import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierSettlementsPage } from "@/features/supplier-settlements/supplier-settlements-page"

export const metadata: Metadata = {
  title: "API 供应商结算",
}

function SettlementsFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
      <div className="h-16 animate-pulse rounded-xl bg-muted" />
      <div className="h-24 animate-pulse rounded-2xl bg-muted" />
      <div className="h-72 animate-pulse rounded-2xl bg-muted" />
    </div>
  )
}

/**
 * SPA 壳：URL 恢复 view / supplier / period / status / differenceType / preview / statementId / section / role。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<SettlementsFallback />}>
      <SupplierSettlementsPage />
    </Suspense>
  )
}
