import type { Metadata } from "next"
import { Suspense } from "react"

import { IntegrationErrorTaskDetailPage } from "@/features/integration-errors/integration-error-detail-page"

export const metadata: Metadata = {
  title: "接口错误任务",
}

function Fallback() {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
      <div className="h-80 animate-pulse rounded-2xl bg-muted" />
    </div>
  )
}

type PageProps = {
  params: Promise<{ taskId: string }>
}

export default async function Page({ params }: PageProps) {
  const { taskId } = await params
  return (
    <Suspense fallback={<Fallback />}>
      <IntegrationErrorTaskDetailPage taskId={taskId} />
    </Suspense>
  )
}
