import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierApiConnectionSettingsPage } from "@/features/supplier-api-connections/pages/supplier-api-connection-settings-page"

export const metadata: Metadata = { title: "API 设置" }

function SettingsFallback() {
    return (
        <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:gap-4 md:px-6 md:py-5">
            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
            <div className="h-10 w-72 animate-pulse rounded-lg bg-muted" />
            <div className="h-40 animate-pulse rounded-xl bg-muted" />
            <div className="h-64 animate-pulse rounded-lg bg-muted" />
        </div>
    )
}

export default function Page() {
    return (
        <Suspense fallback={<SettingsFallback />}>
            <SupplierApiConnectionSettingsPage />
        </Suspense>
    )
}
