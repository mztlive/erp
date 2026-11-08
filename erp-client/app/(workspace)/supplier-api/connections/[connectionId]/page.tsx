import { PagePulseBars } from "@/components/layout/page-pulse"
import type { Metadata } from "next"
import { Suspense } from "react"

import { SupplierApiConnectionSettingsPage } from "@/features/supplier-api-connections/pages/supplier-api-connection-settings-page"

export const metadata: Metadata = { title: "API 设置" }

function SettingsFallback() {
    return (
        <PagePulseBars
            bars={[
                "h-10 w-56 animate-pulse rounded-lg bg-muted",
                "h-10 w-72 animate-pulse rounded-lg bg-muted",
                "h-40 animate-pulse rounded-xl bg-muted",
                "h-64 animate-pulse rounded-lg bg-muted",
            ]}
        />
    )
}

export default function Page() {
    return (
        <Suspense fallback={<SettingsFallback />}>
            <SupplierApiConnectionSettingsPage />
        </Suspense>
    )
}
