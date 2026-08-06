import type { Metadata } from "next"
import { Suspense } from "react"

import { ImportOpeningPage } from "@/features/import-opening/import-opening-page"

export const metadata: Metadata = {
  title: "导入与期初",
}

function ImportOpeningFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
      <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
      <div className="h-24 animate-pulse rounded-lg bg-muted" />
      <div className="h-64 animate-pulse rounded-lg bg-muted" />
    </div>
  )
}

/**
 * SPA 壳：URL 由客户端恢复 environment / batchId / section / 问题筛选等。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<ImportOpeningFallback />}>
      <ImportOpeningPage />
    </Suspense>
  )
}
