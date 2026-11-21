"use client"

import { useEffect } from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "@/lib/navigation"

/** 兼容旧管理地址，统一转入供应商资料。 */
export function SupplierPortalRedirectPage({
    section,
}: {
    section: "portal-accounts" | "portal-catalog" | "portal-applications"
}) {
    const router = useRouter()
    const search = useSearchParams()
    const supplierId = search.get("supplier_id")
    const href = supplierId
        ? `/master-data/suppliers/${encodeURIComponent(supplierId)}?section=${section}`
        : "/master-data/suppliers"

    useEffect(() => {
        router.replace(href)
    }, [href, router])

    return (
        <p className="p-5 text-xs text-muted-foreground">
            管理功能已归入
            <Link
                id="supplier-portal-open-supplier-profile"
                href={href}
                className="underline"
            >
                供应商资料
            </Link>
            ，正在跳转…
        </p>
    )
}
