import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { CustomerReceivablesPage } from "@/features/customer-receivables/customer-receivables-page"

export const metadata: Metadata = {
  title: "客户往来",
}

function CustomerAccountsFallback() {
  return (
    <PageScaffold>
      <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="h-20 animate-pulse rounded-lg bg-muted" />
        ))}
      </div>
      <div className="h-12 animate-pulse rounded-lg bg-muted" />
      <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
    </PageScaffold>
  )
}

/**
 * SPA 壳：URL 查询由客户端 useSearchParams 恢复
 * view / counterpartyId / customerId / q / sessionId / returnTo。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<CustomerAccountsFallback />}>
      <CustomerReceivablesPage />
    </Suspense>
  )
}
