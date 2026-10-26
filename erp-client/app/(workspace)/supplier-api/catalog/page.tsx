import type { Metadata } from "next"
import { Suspense } from "react"
import { SupplierCatalogPage } from "@/features/supplier-api-connections/pages/supplier-catalog-page"

export const metadata: Metadata = { title: "供应商品库" }

export default function Page() {
    return (
        <Suspense
            fallback={
                <p className="p-6 text-sm text-muted-foreground">
                    正在读取供应商品库…
                </p>
            }
        >
            <SupplierCatalogPage />
        </Suspense>
    )
}
