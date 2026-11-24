"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { useIsMutating } from "@tanstack/react-query"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
} from "@/components/business"
import {
    WorkspaceTaskPane,
    type WorkspaceTaskPaneProps,
} from "@/components/business/workspace-task-pane"
import { buttonVariants } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"
import { InquiryCreateView } from "../components/inquiry-create-view"
import {
    InquiryProcurementView,
    InquiryTaskHeader,
} from "../components/inquiry-procurement-view"
import { InquirySalesView } from "../components/inquiry-sales-view"
import { useInquiry } from "../hooks/queries"
import {
    useInquiryCommandResolution,
    type InquiryCommandResolution,
} from "../hooks/use-inquiry-command"
import { resolveInquiryBackLink } from "../lib/navigation"
import { inquiryTaskCompletion } from "../lib/task-completion"

export function SalesInquiryDetailPage({
    inquiryId = "",
    procurement = false,
    workItemId,
    embedded = false,
    onTaskCompleted,
    taskHeaderActions,
    TaskPane = WorkspaceTaskPane,
}: {
    inquiryId?: string
    procurement?: boolean
    workItemId?: string
    embedded?: boolean
    onTaskCompleted?: () => void
    taskHeaderActions?: React.ReactNode
    TaskPane?: React.ComponentType<WorkspaceTaskPaneProps>
}) {
    const search = useSearchParams()
    const router = useRouter()
    const mutating = useIsMutating()
    const returnedTask = React.useRef<string | null>(null)
    const taskId =
        workItemId ??
        search.get("task_id") ??
        search.get("workItemId") ??
        undefined
    const returnTo = search.get("returnTo")
    const copyFromId = !inquiryId ? (search.get("copyFrom") ?? "") : ""
    const profile = useAccountProfileQuery()
    const taskKey = `${inquiryId}:${taskId ?? ""}`
    const [completion, setCompletion] = React.useState<{
        key: string
        message: string
    } | null>(null)
    const completedTask = React.useRef<string | null>(null)
    const onResolved = React.useCallback(
        (resolution: InquiryCommandResolution) => {
            if (!procurement) return
            const result = inquiryTaskCompletion(inquiryId, resolution)
            if (!result || result.taskId !== taskId) return
            // 同步阻止提交回调中的手动刷新，不依赖 React 提交状态更新的时机。
            completedTask.current = taskKey
            setCompletion({ key: taskKey, message: result.message })
        },
        [inquiryId, procurement, taskId, taskKey],
    )
    useInquiryCommandResolution(
        profile.data?.userid ?? "",
        inquiryId,
        onResolved,
    )
    const taskFinished = procurement && completion?.key === taskKey
    const query = useInquiry(
        inquiryId,
        procurement,
        taskId,
        Boolean(inquiryId) && !taskFinished,
    )

    React.useEffect(() => {
        if (!taskFinished || mutating > 0 || returnedTask.current === taskKey)
            return
        // 等待表单卸载和成功命令刷新完成，再让工作区按已保存状态关闭页面。
        const frame = requestAnimationFrame(() => {
            returnedTask.current = taskKey
            if (embedded && onTaskCompleted) {
                onTaskCompleted()
                return
            }
            const host = workspaceHost()
            if (host && isWorkspaceFrame()) {
                window.dispatchEvent(new Event("erp-workspace-form-state"))
                host.changed(window)
                host.closeCurrent(window, "/procurement/inquiries")
            } else {
                router.replace("/procurement/inquiries")
            }
        })
        return () => cancelAnimationFrame(frame)
    }, [embedded, mutating, onTaskCompleted, router, taskFinished, taskKey])

    const detailTitle = procurement ? "采购询价" : "销售询价"
    const renderEmbeddedState = (
        body: React.ReactNode,
        description?: string,
    ) => (
        <TaskPane
            aria-label={detailTitle}
            archetype="action"
            header={
                <InquiryTaskHeader
                    title={procurement ? "询价采购处理" : detailTitle}
                    meta={
                        description ? (
                            <span className="text-xs text-muted-foreground">
                                {description}
                            </span>
                        ) : undefined
                    }
                    actions={taskHeaderActions}
                />
            }
        >
            <div className="min-w-0 py-6">{body}</div>
        </TaskPane>
    )

    if (!profile.data) {
        return !embedded ? (
            <DetailWorkspaceSkeleton
                title={
                    inquiryId
                        ? detailTitle
                        : copyFromId
                          ? "复制为新询价"
                          : "新建销售询价"
                }
                description="正在核对账号权限…"
            />
        ) : (
            renderEmbeddedState(
                <p
                    className="text-body-compact text-muted-foreground"
                    role="status"
                >
                    正在核对账号权限…
                </p>,
            )
        )
    }

    if (!inquiryId) {
        return (
            <InquiryCreateView
                copyFromId={copyFromId}
                returnTo={returnTo}
                userId={profile.data.userid}
                ownerName={(
                    profile.data.name ||
                    profile.data.account ||
                    ""
                ).trim()}
                permissions={profile.data.permissions}
            />
        )
    }

    if (taskFinished) {
        const message =
            completion?.message || "本轮询价处理已完成，正在返回列表…"
        return embedded ? (
            renderEmbeddedState(
                <p
                    className="text-body-compact text-muted-foreground"
                    role="status"
                >
                    {message}
                </p>,
            )
        ) : (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title={detailTitle}
                    back={resolveInquiryBackLink({
                        id: "sales-inquiry-completed-back",
                        procurement,
                        returnTo,
                    })}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <p
                            className="text-body-compact text-muted-foreground"
                            role="status"
                        >
                            {message}
                        </p>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (query.isPending) {
        return embedded ? (
            renderEmbeddedState(
                <p
                    className="text-body-compact text-muted-foreground"
                    role="status"
                >
                    正在读取询价资料…
                </p>,
            )
        ) : (
            <DetailWorkspaceSkeleton
                title={detailTitle}
                description="正在读取询价资料…"
            />
        )
    }

    const errorBack = resolveInquiryBackLink({
        id: "sales-inquiry-error-back",
        procurement,
        returnTo,
    })

    if (query.isError) {
        const failure = (
            <BusinessFailureState
                id="sales-inquiry-detail-retry"
                title="询价资料加载失败"
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
        return embedded ? (
            renderEmbeddedState(
                <div className="space-y-4">
                    {failure}
                    {errorBack.href ? (
                        <Link
                            id={errorBack.id}
                            href={errorBack.href}
                            className={buttonVariants({ variant: "outline" })}
                        >
                            {errorBack.label}
                        </Link>
                    ) : null}
                </div>,
            )
        ) : (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={detailTitle} back={errorBack} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        {failure}
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!query.data) {
        const empty = (
            <BusinessEmptyState
                kind="no-data"
                title="未找到询价单"
                description="该询价单可能不存在或当前账号无权查看。"
            />
        )
        return embedded ? (
            renderEmbeddedState(empty)
        ) : (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={detailTitle} back={errorBack} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        {empty}
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const handleSaved = () => {
        if (completedTask.current !== taskKey) void query.refetch()
    }

    if (procurement && "work_items" in query.data) {
        return (
            <InquiryProcurementView
                key={`procurement:${inquiryId}:${taskId ?? ""}`}
                inquiry={query.data}
                taskId={taskId}
                embedded={embedded}
                userId={profile.data.userid}
                permissions={profile.data.permissions}
                returnTo={returnTo}
                onSaved={handleSaved}
                taskHeaderActions={taskHeaderActions}
                TaskPane={TaskPane}
            />
        )
    }

    if (!procurement && "sales_owner_user_id" in query.data) {
        return (
            <InquirySalesView
                key={`sales:${inquiryId}`}
                inquiry={query.data}
                userId={profile.data.userid}
                permissions={profile.data.permissions}
                returnTo={returnTo}
                onSaved={handleSaved}
            />
        )
    }

    return null
}
