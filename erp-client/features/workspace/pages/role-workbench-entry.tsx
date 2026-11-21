"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import { useRouter } from "@/lib/navigation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    PageScaffold,
    PageHeader,
    BusinessFailureState,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ROLE_WORKBENCHES } from "@/lib/role-workbenches"
import { hasPermission } from "@/lib/permissions"

export function preferenceKey(userid: string) {
    return `erp:role-workbench:${userid}`
}
export function readPreference(userid: string): {
    last?: string
    default?: string
} {
    try {
        return (
            JSON.parse(localStorage.getItem(preferenceKey(userid)) || "{}") ??
            {}
        )
    } catch {
        return {}
    }
}
export function savePreference(userid: string, id: string, asDefault = false) {
    try {
        const old = readPreference(userid)
        localStorage.setItem(
            preferenceKey(userid),
            JSON.stringify({
                ...old,
                last: id,
                ...(asDefault ? { default: id } : {}),
            }),
        )
        return true
    } catch {
        /* 禁止存储时仍允许本次导航。 */
        return false
    }
}
export function RoleWorkbenchEntry({
    settings = false,
}: {
    settings?: boolean
}) {
    const query = useAccountProfileQuery()
    const profile = query.isError ? undefined : query.data
    const router = useRouter()
    const [ready, setReady] = useState(false)
    useEffect(() => {
        if (!profile) return
        if (settings) {
            setReady(true)
            return
        }
        const ids = profile.workbenches ?? []
        const preference = readPreference(profile.userid)
        const selected = [
            preference.last,
            preference.default,
            ids.length === 1 ? ids[0] : undefined,
        ].find((id) => id && ids.includes(id))
        if (selected) router.replace(`/workspace/roles/${selected}`)
        else if (
            !ids.length &&
            hasPermission(profile.permissions, "work_item:list")
        )
            router.replace("/workspace/tasks")
        else setReady(true)
    }, [profile, router, settings])
    if (query.isError)
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </PageScaffold>
        )
    return (
        <PageScaffold>
            <PageHeader title={settings ? "个人首页设置" : "选择默认工作台"} />
            {!ready ? (
                <p role="status">正在读取岗位配置…</p>
            ) : profile?.workbenches?.length ? (
                <div className="grid gap-4 md:grid-cols-3">
                    {ROLE_WORKBENCHES.filter((item) =>
                        profile.workbenches?.includes(item.id),
                    ).map((item) => (
                        <section
                            key={item.id}
                            className="rounded-xl border p-5"
                        >
                            <h2 className="text-section-title font-semibold tracking-tight">
                                {item.name}
                            </h2>
                            <p className="my-4 text-xs text-muted-foreground">
                                {item.description}
                            </p>
                            <Button
                                id={`workbench-select-${item.id.toLowerCase()}`}
                                onClick={() => {
                                    savePreference(
                                        profile.userid,
                                        item.id,
                                        true,
                                    )
                                    router.push(`/workspace/roles/${item.id}`)
                                }}
                            >
                                设为默认并进入
                            </Button>
                        </section>
                    ))}
                </div>
            ) : (
                <p>暂无可用工作台，请联系管理员配置岗位及业务权限。</p>
            )}
            {profile &&
                hasPermission(profile.permissions, "work_item:list") && (
                    <Link
                        id="workbench-entry-todos"
                        href="/workspace/tasks"
                        className="text-body-compact underline"
                    >
                        待我处理
                    </Link>
                )}
        </PageScaffold>
    )
}
