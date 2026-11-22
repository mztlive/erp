import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { InvoiceRequestsPage } from "@/features/invoice-requests/components/request-page"

export const metadata: Metadata = {
    title: "开票申请",
}

export default function Page() {
    return (
        <Suspense
            fallback={
                <PageScaffold>
                    <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                    <div className="h-12 animate-pulse rounded-lg bg-muted" />
                    <div className="h-96 animate-pulse rounded-lg bg-muted" />
                </PageScaffold>
            }
        >
            <InvoiceRequestsPage />
        </Suspense>
    )
}
