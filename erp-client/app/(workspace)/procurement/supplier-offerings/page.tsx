import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierOfferingsPage } from "@/features/supplier-offerings/pages/supplier-offerings-page"

export const metadata: Metadata = {
    title: "供应商供货目录",
}

function Fallback() {
    return (
        <PagePulseBars
            className="gap-4"
            bars={[
                "h-10 w-48 animate-pulse rounded-lg bg-muted",
                "h-20 animate-pulse rounded-lg bg-muted",
                "h-80 animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

export default function Page() {
    return (
        <Suspense fallback={<Fallback />}>
            <SupplierOfferingsPage />
        </Suspense>
    )
}
