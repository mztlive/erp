import type { Metadata } from "next"
import { Suspense } from "react"

import { PageHeader } from "@/components/business"
import { UnifiedTaskQueuePage } from "@/features/unified-task-queue/unified-task-queue-page"

export const metadata: Metadata = {
  title: "待办队列",
}

export default function Page() {
  return (
    <Suspense
      fallback={
        <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
          <PageHeader title="统一待办队列" description="正在加载…" />
        </div>
      }
    >
      <UnifiedTaskQueuePage />
    </Suspense>
  )
}
