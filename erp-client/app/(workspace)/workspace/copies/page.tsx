import type { Metadata } from "next"
import { Suspense } from "react"
import { ApprovalCopiesPage } from "@/features/approval-copies/page"
import { WorkspaceHomeSkeleton } from "@/features/workspace/components/workspace-home-skeleton"

export const metadata: Metadata = { title: "抄送给我" }

export default function Page() {
    return (
        <Suspense fallback={<WorkspaceHomeSkeleton />}>
            <ApprovalCopiesPage />
        </Suspense>
    )
}
