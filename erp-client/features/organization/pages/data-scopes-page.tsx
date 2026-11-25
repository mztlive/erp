"use client"
import Link from "next/link"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { ListWorkspacePage } from "@/components/business"
/** 数据范围统一在人员上下文中设置。 */
export function DataScopesPage() {
    return (
        <ListWorkspacePage title="人员数据范围" className="gap-4">
            <div className="space-y-3 rounded-lg border border-border/60 bg-background p-5 text-body-compact">
                <p>请选择需要设置的人员，再按业务及操作设置其唯一数据范围。</p>
                <PermissionGuard permission="admin:list">
                    <Link
                        id="person-scope-accounts-entry"
                        href="/system/accounts"
                        className="text-primary"
                    >
                        进入人员账号
                    </Link>
                </PermissionGuard>
            </div>
        </ListWorkspacePage>
    )
}
