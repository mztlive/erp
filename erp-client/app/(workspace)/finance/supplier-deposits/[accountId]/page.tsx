import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { SupplierDepositDetailPage } from "@/features/supplier-deposits/components/detail-page"

export const metadata: Metadata = { title: "预存款账户" }

export default function Page() {
    return (
        <Suspense
            fallback={<DetailWorkspaceSkeleton title="预存款账户" summary />}
        >
            <SupplierDepositDetailPage />
        </Suspense>
    )
}
