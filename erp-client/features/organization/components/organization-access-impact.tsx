"use client"
import Link from "next/link"
import type { OrganizationChangeDraft } from "../lib/change-payload"
import type { OrganizationStateView } from "../types"
export function OrganizationAccessImpact({
    draft,
}: {
    draft: OrganizationChangeDraft
    view: OrganizationStateView
}) {
    return (
        <div className="space-y-2 rounded-md bg-muted/40 p-3 text-sm">
            <p className="font-medium">权限影响</p>
            <p className="text-xs text-muted-foreground">
                所属部门变更会重算此人已配置的“所属部门”范围。角色决定可执行的操作，人员数据范围决定可处理的数据；指定部门范围不会随所属部门自动变化。
            </p>
            {draft.userId && (
                <Link
                    id="organization-impact-account"
                    className="text-primary"
                    href={`/system/accounts/${encodeURIComponent(draft.userId)}`}
                >
                    查看此人的业务范围
                </Link>
            )}
        </div>
    )
}
