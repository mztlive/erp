import type { Metadata } from "next"
import { Suspense } from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { SupplierApiConnectionSettingsPage } from "@/features/supplier-api-connections/pages/supplier-api-connection-settings-page"

export const metadata: Metadata = { title: "平台连接设置" }

export default function Page() {
    return (
        <Suspense
            fallback={
                <DetailWorkspaceSkeleton
                    title="平台连接设置"
                    description="正在读取连接状态与接口配置…"
                />
            }
        >
            <SupplierApiConnectionSettingsPage />
        </Suspense>
    )
}
