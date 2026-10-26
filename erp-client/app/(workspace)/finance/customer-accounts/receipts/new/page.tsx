import type { Metadata } from "next"
import { Suspense } from "react"
import { PageScaffold } from "@/components/business"
import { CustomerReceiptRegistrationPage } from "@/features/customer-receivables/pages/customer-receipt-registration-page"

export const metadata: Metadata = { title: "登记回款" }

export default function Page() {
    return (
        <Suspense
            fallback={
                <PageScaffold>
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </PageScaffold>
            }
        >
            <CustomerReceiptRegistrationPage />
        </Suspense>
    )
}
