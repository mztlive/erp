import type { Metadata } from "next"
import { Suspense } from "react"

import { ProcurementConfirmationPage } from "@/features/procurement-confirmation/procurement-confirmation-page"

export const metadata: Metadata = {
  title: "采购二次确认",
}

function ProcurementConfirmFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
      <div className="h-24 animate-pulse rounded-2xl bg-muted" />
      <div className="grid gap-4 xl:grid-cols-[minmax(0,2fr)_minmax(16rem,1fr)]">
        <div className="h-80 animate-pulse rounded-2xl bg-muted" />
        <div className="h-64 animate-pulse rounded-2xl bg-muted" />
      </div>
    </div>
  )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 读取并恢复
 * scope / due / sort / currentWorkItemId / queueContextId / autoNext。
 * 业务数据不在服务端 fetch。
 */
export default function ProcurementConfirmPage() {
  return (
    <Suspense fallback={<ProcurementConfirmFallback />}>
      <ProcurementConfirmationPage />
    </Suspense>
  )
}
