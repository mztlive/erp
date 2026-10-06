"use client"
import type { ReactNode } from "react"
import Link from "next/link"
import { usePathname } from "next/navigation"
import { PortalSurface } from "@/features/supplier-portal/components/surface"
import { usePortalAdminAccess } from "../hooks"
const entries = [
    ["applications", "供应商申请", "supplier_portal_request:list"],
    ["accounts", "门户账号", "supplier_portal_account:list"],
    ["catalog", "SKU定向开放", "supplier_portal_catalog:list"],
] as const
export function PortalAdminFrame({
    title,
    children,
}: {
    title: string
    children: ReactNode
}) {
    const pathname = usePathname()
    const access = usePortalAdminAccess()
    return (
        <PortalSurface
            title={title}
            description="采购确认供应商商务变更；新品入库与销售上架分别执行。"
            actions={
                access.can("product:list") ? (
                    <>
                        <Link
                            id="supplier-portal-admin-unlisted"
                            className="text-sm text-primary"
                            href="/master-data/products?productListingStatus=unlisted"
                        >
                            全部未上架商品
                        </Link>
                        <Link
                            id="supplier-portal-admin-partially-listed"
                            className="text-sm text-primary"
                            href="/master-data/products?productListingStatus=partially_listed"
                        >
                            还有未上架规格
                        </Link>
                    </>
                ) : null
            }
        >
            <nav aria-label="供应商门户管理" className="flex flex-wrap gap-2">
                {entries
                    .filter(([, , permission]) => access.can(permission))
                    .map(([path, label]) => (
                        <Link
                            id={`supplier-portal-admin-nav-${path}`}
                            key={path}
                            href={`/procurement/supplier-portal/${path}`}
                            className={`rounded-md px-3 py-2 text-sm ${pathname.includes(`/${path}`) ? "bg-primary text-primary-foreground" : "bg-muted hover:bg-accent"}`}
                        >
                            {label}
                        </Link>
                    ))}
            </nav>
            {children}
        </PortalSurface>
    )
}
