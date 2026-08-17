import type { Metadata } from "next"
import { Suspense } from "react"

import { PageHeader, PageScaffold } from "@/components/business"
import { UnifiedTaskQueuePage } from "@/features/unified-task-queue/pages/unified-task-queue-page"

export const metadata: Metadata = {
    title: "待办队列",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <PageScaffold>
                    <PageHeader title="统一待办队列" description="正在加载…" />
                </PageScaffold>
            }
        >
            <UnifiedTaskQueuePage />
        </Suspense>
    )
}
