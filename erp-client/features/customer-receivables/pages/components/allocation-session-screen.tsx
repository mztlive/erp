"use client"

import type * as React from "react"
import { getErrorMessage } from "@/lib/api/errors"

import { BusinessFailureState, PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import { AllocationSessionPanel } from "@/features/customer-receivables/components/allocation-session-panel"
import type {
    AllocationSessionView,
    PostAllocationResult,
} from "@/features/customer-receivables/types"

type AllocationSessionScreenProps = {
    isPending: boolean
    error?: unknown
    onRetry?: () => void
    session: AllocationSessionView | null | undefined
    onBackToList: () => void
    onClose: () => void
    onPosted: (
        result: Extract<PostAllocationResult, { status: "succeeded" }>,
    ) => void
    canOperate?: boolean
    permissionReason?: string
    workItemId?: string
    expectedTaskVersion?: string
    taskReceivableAccountId?: string
    /** 已由对象详情承载页面框架时，只渲染会话内容。 */
    embedded?: boolean
    /** 工作台页内作业不展示会话自己的返回入口。 */
    hideSessionClose?: boolean
    /** 提交结果区关闭按钮文案；缺省为「返回列表」。 */
    closeLabel?: string
}

/** 核销会话全屏态：加载骨架 / 失效提示 / 会话面板。 */
export function AllocationSessionScreen({
    isPending,
    error,
    onRetry,
    session,
    onBackToList,
    onClose,
    onPosted,
    canOperate = true,
    permissionReason,
    workItemId,
    expectedTaskVersion,
    taskReceivableAccountId,
    embedded = false,
    hideSessionClose = false,
    closeLabel,
}: AllocationSessionScreenProps) {
    const wrap = (content: React.ReactNode) =>
        embedded ? (
            content
        ) : (
            <PageScaffold density="compact" className="gap-6">
                {content}
            </PageScaffold>
        )

    if (isPending) {
        return wrap(
            <div className="flex flex-col gap-4">
                <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
            </div>,
        )
    }
    if (error) {
        return wrap(
            <BusinessFailureState
                kind="business"
                title="回款登记未能加载"
                description={getErrorMessage(
                    error,
                    "请重试加载，已暂存内容仍保留。",
                )}
                action={
                    <div className="flex gap-2">
                        <Button
                            id="customer-receivables-session-retry"
                            onClick={onRetry}
                        >
                            重试加载
                        </Button>
                        <Button
                            id="customer-receivables-session-error-back"
                            variant="outline"
                            onClick={onBackToList}
                        >
                            返回列表
                        </Button>
                    </div>
                }
            />,
        )
    }
    if (session?.mode === "receipt" && session.status === "posted") {
        return wrap(
            <div className="space-y-4">
                <h2 className="text-lg font-semibold">回款已登记</h2>
                <p>
                    {session.existingFactNo} ·{" "}
                    {session.serverStatus === "posted"
                        ? "已过账"
                        : session.serverStatus === "reversed"
                          ? "已冲正"
                          : "审批中"}
                </p>
                <Button
                    id="customer-receivables-session-complete-back"
                    onClick={onBackToList}
                >
                    返回列表
                </Button>
            </div>,
        )
    }
    if (!session) {
        return wrap(
            <BusinessFailureState
                kind="business"
                title="本次核销无效"
                description="本页未找到暂存内容。已保存的回款可从列表重新打开；未保存的登记请重新开始。"
                action={
                    <Button
                        id="customer-receivables-session-invalid-back"
                        type="button"
                        onClick={onBackToList}
                    >
                        {hideSessionClose ? "重试" : "返回列表"}
                    </Button>
                }
            />,
        )
    }
    return wrap(
        <AllocationSessionPanel
            key={session.draftSessionId}
            session={session}
            onClose={onClose}
            onPosted={onPosted}
            canOperate={canOperate}
            permissionReason={permissionReason}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            taskReceivableAccountId={taskReceivableAccountId}
            hideSessionClose={hideSessionClose}
            closeLabel={closeLabel}
        />,
    )
}
