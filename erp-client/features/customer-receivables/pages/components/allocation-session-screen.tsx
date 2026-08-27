"use client"

import type * as React from "react"

import { BusinessFailureState, PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import { AllocationSessionPanel } from "@/features/customer-receivables/components/allocation-session-panel"
import type { AllocationSessionView } from "@/features/customer-receivables/types"

type AllocationSessionScreenProps = {
    isPending: boolean
    session: AllocationSessionView | null | undefined
    onBackToList: () => void
    onClose: () => void
    onPosted: () => void
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
        embedded ? content : <PageScaffold>{content}</PageScaffold>

    if (isPending) {
        return wrap(
            <div className="flex flex-col gap-4">
                <div className="h-10 w-64 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
            </div>,
        )
    }
    if (!session) {
        return wrap(
            <BusinessFailureState
                kind="business"
                title="本次核销无效"
                description="本次核销已失效，请重新开始。"
                action={
                    <Button type="button" onClick={onBackToList}>
                        {hideSessionClose ? "重试" : "返回列表"}
                    </Button>
                }
            />,
        )
    }
    return wrap(
        <AllocationSessionPanel
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
