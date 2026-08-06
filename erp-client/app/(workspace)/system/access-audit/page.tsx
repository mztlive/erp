import type { Metadata } from "next"
import { Suspense } from "react"

import { AccessAuditPage } from "@/features/access-audit/access-audit-page"

export const metadata: Metadata = {
  title: "权限与审计",
}

function AccessAuditFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
      <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
      <div className="h-12 animate-pulse rounded-xl bg-muted" />
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="h-20 animate-pulse rounded-lg bg-muted" />
        ))}
      </div>
      <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
    </div>
  )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 恢复
 * view=roles|users|scopes|fields|audit 与 subjectId / eventId 等。
 * 业务数据不在服务端 fetch。
 */
export default function AccessAuditRoutePage() {
  return (
    <Suspense fallback={<AccessAuditFallback />}>
      <AccessAuditPage />
    </Suspense>
  )
}
