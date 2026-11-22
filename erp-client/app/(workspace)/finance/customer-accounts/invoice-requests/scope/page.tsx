import type { Metadata } from "next"
import { Suspense } from "react"

import { ScopePagePulse } from "@/components/layout/page-pulse"
import { InvoiceRequestScopePage } from "@/features/invoice-requests/components/scope-page"

export const metadata: Metadata = {
    title: "开票申请汇总",
}

export default function Page() {
    return (
        <Suspense fallback={<ScopePagePulse />}>
            <InvoiceRequestScopePage />
        </Suspense>
    )
}
