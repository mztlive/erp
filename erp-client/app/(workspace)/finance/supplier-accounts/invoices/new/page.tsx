import type { Metadata } from "next"
import { Suspense } from "react"

import { PageScaffold } from "@/components/business"
import { SupplierInvoiceRegistrationPage } from "@/features/supplier-payables/pages/supplier-invoice-registration-page"

export const metadata: Metadata = { title: "登记进项发票" }

/** 进项发票登记独立路由；查询、草稿和业务操作全部由客户端负责。 */
export default function Page() {
    return (
        <Suspense
            fallback={
                <PageScaffold>
                    <div className="h-72 animate-pulse rounded-lg bg-muted" />
                </PageScaffold>
            }
        >
            <SupplierInvoiceRegistrationPage />
        </Suspense>
    )
}
