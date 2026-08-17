import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierOfferingsPage } from "@/features/supplier-offerings/pages/supplier-offerings-page"

export const metadata: Metadata = {
    title: "供应商供给",
}

function Fallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:px-6 md:py-5">
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-20 animate-pulse rounded-lg bg-muted" />
            <div className="h-80 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

export default function Page() {
    return (
        <Suspense fallback={<Fallback />}>
            <SupplierOfferingsPage />
        </Suspense>
    )
}
