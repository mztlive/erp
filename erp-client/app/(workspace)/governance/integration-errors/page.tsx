import type { Metadata } from "next"
import { Suspense } from "react"

import { IntegrationErrorsPage } from "@/features/integration-errors/integration-errors-page"

export const metadata: Metadata = {
  title: "接口错误与对账中心",
}

function IntegrationErrorsFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
      <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
      <div className="h-16 animate-pulse rounded-lg bg-muted" />
      <div className="grid gap-4 xl:grid-cols-[minmax(0,38fr)_minmax(0,62fr)]">
        <div className="h-80 animate-pulse rounded-lg bg-muted" />
        <div className="h-80 animate-pulse rounded-lg bg-muted" />
      </div>
    </div>
  )
}

/**
 * SPA 壳：URL 恢复 view/mode/errorClass/environment/owner/taskId/differenceId/
 * queueContextId/autoNext/resolveWorkItemId。业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<IntegrationErrorsFallback />}>
      <IntegrationErrorsPage />
    </Suspense>
  )
}
