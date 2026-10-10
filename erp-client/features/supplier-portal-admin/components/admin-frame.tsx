"use client"
import type { ReactNode } from "react"
import Link from "next/link"
import { usePathname } from "next/navigation"
import { PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { ArrowUpRightIcon } from "lucide-react"
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
        <PageScaffold density="compact" className={listWorkspaceStyles.page}>
            <ListWorkspaceHeader
                eyebrow="采购与履约 · 供应商门户管理"
                title={title}
                description="采购确认供应商商务变更；新品入库与销售上架分别执行。"
            >
                {access.can("product:list") ? (
                    <div className="flex flex-wrap justify-end gap-2">
                        <Button
                            id="supplier-portal-admin-unlisted"
                            variant="outline"
                            render={
                                <Link href="/master-data/products?productListingStatus=unlisted" />
                            }
                        >
                            全部未上架商品
                            <ArrowUpRightIcon data-icon="inline-end" />
                        </Button>
                        <Button
                            id="supplier-portal-admin-partially-listed"
                            variant="outline"
                            render={
                                <Link href="/master-data/products?productListingStatus=partially_listed" />
                            }
                        >
                            还有未上架规格
                            <ArrowUpRightIcon data-icon="inline-end" />
                        </Button>
                    </div>
                ) : null}
            </ListWorkspaceHeader>
            <ListWorkspaceViews
                ariaLabel="供应商门户管理"
                items={entries
                    .filter(([, , permission]) => access.can(permission))
                    .map(([path, label]) => ({
                        id: `supplier-portal-admin-nav-${path}`,
                        label,
                        href: `/procurement/supplier-portal/${path}`,
                        active: pathname.includes(`/${path}`),
                    }))}
            />
            <div className="flex min-w-0 flex-1 flex-col gap-6">{children}</div>
        </PageScaffold>
    )
}
