"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"

import {
    BusinessFailureState,
    PageScaffold,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useRouter } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { ROLE_WORKBENCHES } from "@/lib/role-workbenches"

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
            <PageScaffold
                density="compact"
                className={`${detailWorkspaceStyles.page} min-h-0 overflow-auto`}
            >
                <BusinessFailureState
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </PageScaffold>
        )

    const availableWorkbenches = ROLE_WORKBENCHES.filter((item) =>
        profile?.workbenches?.includes(item.id),
    )
    const defaultWorkbenchId = profile
        ? readPreference(profile.userid).default
        : undefined

    return (
        <PageScaffold
            density="compact"
            className={`${detailWorkspaceStyles.page} min-h-0 overflow-auto`}
        >
            <header className="sticky top-0 z-20 flex min-h-12 shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/60 bg-card py-2">
                <div className="flex min-w-0 items-baseline gap-2.5">
                    <h1 className={detailWorkspaceStyles.title}>
                        {settings ? "个人首页设置" : "选择默认工作台"}
                    </h1>
                    <span className="text-xs text-muted-foreground">
                        选择登录后默认进入的岗位工作台
                    </span>
                </div>
                {profile &&
                    hasPermission(profile.permissions, "work_item:list") && (
                        <Button
                            id="workbench-entry-todos"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.subtleButton}
                            render={<Link href="/workspace/tasks" />}
                        >
                            待我处理
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    )}
            </header>

            <section className="py-5" aria-label="可用岗位工作台">
                {!ready ? (
                    <p
                        role="status"
                        className="text-body-compact text-muted-foreground"
                    >
                        正在读取岗位配置…
                    </p>
                ) : availableWorkbenches.length > 0 && profile ? (
                    <div className="grid gap-4 sm:grid-cols-2 @[960px]/workspace:grid-cols-3">
                        {availableWorkbenches.map((item) => {
                            const isDefault = defaultWorkbenchId === item.id
                            return (
                                <section
                                    key={item.id}
                                    className="flex flex-col justify-between gap-4 rounded-lg border border-border/60 bg-card p-5 transition-colors hover:border-border"
                                >
                                    <div className="space-y-1.5">
                                        <div className="flex items-center justify-between gap-2">
                                            <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                                                {item.name}
                                            </h2>
                                            {isDefault ? (
                                                <span className="rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-soft-foreground">
                                                    当前默认
                                                </span>
                                            ) : null}
                                        </div>
                                        <p className="text-xs text-muted-foreground">
                                            {item.description}
                                        </p>
                                    </div>
                                    <div className="flex items-center gap-2 pt-1">
                                        <Button
                                            id={`workbench-select-${item.id.toLowerCase()}`}
                                            size="sm"
                                            onClick={() => {
                                                savePreference(
                                                    profile.userid,
                                                    item.id,
                                                    true,
                                                )
                                                router.push(
                                                    `/workspace/roles/${item.id}`,
                                                )
                                            }}
                                        >
                                            设为默认并进入
                                        </Button>
                                    </div>
                                </section>
                            )
                        })}
                    </div>
                ) : (
                    <p className="text-body-compact text-muted-foreground">
                        暂无可用工作台，请联系管理员配置岗位及业务权限。
                    </p>
                )}
            </section>
        </PageScaffold>
    )
}
