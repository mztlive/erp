import type { Metadata } from "next"
import { Suspense } from "react"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"
import { WorkspaceHomePage } from "@/features/workspace/pages/workspace-home-page"

export const metadata: Metadata = { title: "我的工作台" }
export default function Page() {
    return (
        <Suspense fallback={<WorkspaceHomeSkeleton />}>
            <WorkspaceHomePage />
        </Suspense>
    )
}
