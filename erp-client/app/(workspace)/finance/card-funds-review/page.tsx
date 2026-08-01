import type { Metadata } from "next"
import { Suspense } from "react"

import { CardFundsReviewPage } from "@/features/card-funds-review/card-funds-review-page"

export const metadata: Metadata = {
  title: "卡券票款复核",
}

function CardFundsReviewFallback() {
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
 * SPA 壳：URL 由客户端恢复 type / scope / status / currentWorkItemId / queueContextId / autoNext。
 * 业务数据不在服务端 fetch。
 */
export default function Page() {
  return (
    <Suspense fallback={<CardFundsReviewFallback />}>
      <CardFundsReviewPage />
    </Suspense>
  )
}
