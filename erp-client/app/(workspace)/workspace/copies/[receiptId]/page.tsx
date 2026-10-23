import type { Metadata } from "next"
import { Suspense } from "react"
import { ApprovalCopyDetailPage } from "@/features/approval-copies/detail-page"

export const metadata: Metadata = { title: "抄送详情" }

export default async function Page({
    params,
}: {
    params: Promise<{ receiptId: string }>
}) {
    const { receiptId } = await params
    return (
        <Suspense>
            <ApprovalCopyDetailPage id={receiptId} />
        </Suspense>
    )
}
