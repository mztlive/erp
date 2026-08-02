import type { Metadata } from "next"
import { Suspense } from "react"

import { HistoryBackfillPage } from "@/features/history-backfill/history-backfill-page"

export const metadata: Metadata = {
  title: "回填任务详情",
}

function HistoryBackfillJobFallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
      <div className="h-24 animate-pulse rounded-2xl bg-muted" />
      <div className="h-40 animate-pulse rounded-2xl bg-muted" />
    </div>
  )
}

/**
 * SPA 壳：任务对象中心 `/governance/history-backfill/:jobId`。
 * 业务数据不在服务端 fetch。
 */
export default async function Page({
  params,
}: {
  params: Promise<{ jobId: string }>
}) {
  const { jobId } = await params
  return (
    <Suspense fallback={<HistoryBackfillJobFallback />}>
      <HistoryBackfillPage routeJobId={jobId} />
    </Suspense>
  )
}
