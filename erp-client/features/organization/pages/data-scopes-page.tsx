"use client"
import Link from "next/link"
import { PageScaffold, PageHeader } from "@/components/business"
/** 数据范围统一在人员上下文中设置。 */
export function DataScopesPage() {
    return (
        <PageScaffold>
            <PageHeader title="人员数据范围" />
            <div className="space-y-3 rounded-lg border p-5 text-sm">
                <p>请选择需要设置的人员，再按业务及操作设置其唯一数据范围。</p>
                <p className="text-xs text-muted-foreground">
                    角色只提供操作权限。可以让同一人查看部门数据，同时只能修改本人数据。
                </p>
                <Link
                    id="person-scope-accounts-entry"
                    href="/system/accounts"
                    className="text-primary"
                >
                    进入人员账号
                </Link>
            </div>
        </PageScaffold>
    )
}
