import type { Metadata } from "next"
import { Suspense } from "react"
import { ApprovalMaterialPage } from "@/features/approval-workflow/pages/approval-material-page"

export const metadata: Metadata = { title: "审批附件预览" }

export default function Page() {
    return (
        <Suspense fallback={null}>
            <ApprovalMaterialPage />
        </Suspense>
    )
}
