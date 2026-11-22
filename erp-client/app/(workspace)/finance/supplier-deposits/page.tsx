import type { Metadata } from "next"
import { Suspense } from "react"
import { PageScaffold } from "@/components/business"
import { SupplierDepositsPage } from "@/features/supplier-deposits/components/accounts-page"
export const metadata: Metadata = { title: "供应商预存款" }
export default function Page() {
    return (
        <Suspense
            fallback={
                <PageScaffold density="compact">
                    <div className="h-12 animate-pulse rounded-lg bg-muted" />
                    <div className="h-96 animate-pulse rounded-lg bg-muted" />
                </PageScaffold>
            }
        >
            <SupplierDepositsPage />
        </Suspense>
    )
}
