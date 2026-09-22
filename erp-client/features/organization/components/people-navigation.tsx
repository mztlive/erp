"use client"

import Link from "next/link"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

/** 组织与人员的两个工作视图共用入口，按读取权限分别显示。 */
export function PeopleNavigation({
    current,
}: {
    current: "departments" | "accounts"
}) {
    const { data } = useAccountProfileQuery()
    return (
        <nav
            aria-label="组织与人员"
            className="flex shrink-0 flex-wrap gap-2 border-b pb-3"
        >
            {[
                {
                    key: "departments",
                    label: "部门与成员",
                    href: "/system/organization",
                    permission: "org_unit:list",
                },
                {
                    key: "accounts",
                    label: "人员账号",
                    href: "/system/accounts",
                    permission: "admin:list",
                },
            ]
                .filter((item) =>
                    hasPermission(data?.permissions, item.permission),
                )
                .map((item) => (
                    <Link
                        key={item.key}
                        id={`people-navigation-${item.key}`}
                        href={item.href}
                        aria-current={current === item.key ? "page" : undefined}
                        className={`rounded-md px-3 py-2 text-sm ${current === item.key ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-muted"}`}
                    >
                        {item.label}
                    </Link>
                ))}
        </nav>
    )
}
