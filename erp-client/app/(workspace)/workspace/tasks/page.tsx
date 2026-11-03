import type { Metadata } from "next"
import { Suspense } from "react"
import { WorkspaceHomePage } from "@/features/workspace/pages/workspace-home-page"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"

export const metadata: Metadata = { title: "待我处理" }

export default function Page() {
    return (
        <Suspense fallback={<WorkspaceHomeSkeleton />}>
            <WorkspaceHomePage />
        </Suspense>
    )
}
