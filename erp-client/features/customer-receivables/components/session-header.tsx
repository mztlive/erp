"use client"

import Link from "next/link"
import { XIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { formatDateTime } from "@/lib/datetime"
import type { AllocationSessionView } from "@/features/customer-receivables/types"

/** 核销会话页头。工作台页内作业可隐藏返回按钮。 */
export function SessionHeader({
    session,
    isReceipt,
    existing,
    draftSavedAt,
    onRequestClose,
    showClose = true,
}: {
    session: AllocationSessionView
    isReceipt: boolean
    existing: boolean
    draftSavedAt: string | undefined
    onRequestClose: () => void
    showClose?: boolean
}) {
    return (
        <>
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                    <h2 className="font-heading text-lg font-semibold">
                        核销 · {session.counterpartyPartyName}
                    </h2>
                    <p className="text-sm text-muted-foreground">
                        模式：{isReceipt ? "回款核销" : "发票核销"}
                        {existing
                            ? ` · 继续单号 ${session.existingFactNo}`
                            : null}
                        {draftSavedAt
                            ? ` · 草稿已保存 ${formatDateTime(draftSavedAt, "monthDayIntl")}`
                            : " · 未保存草稿"}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        {session.note}
                    </p>
                </div>
                {showClose ? (
                    <Button
                        id="customer-receivables-session-close"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onRequestClose}
                    >
                        <XIcon data-icon="inline-start" aria-hidden="true" />
                        {session.returnContext?.returnTo
                            ? "取消并返回"
                            : "返回列表"}
                    </Button>
                ) : null}
            </div>

            {session.returnContext?.from === "W05" &&
            session.returnContext.returnTo ? (
                <Alert variant="info">
                    <AlertTitle>来自销售单票款区</AlertTitle>
                    <AlertDescription>
                        完成或取消后可回到销售单原入口；筛选与主体在本次核销内保留。
                        <Button
                            id="customer-receivables-session-return-source"
                            type="button"
                            size="sm"
                            variant="link"
                            className="ml-2 h-auto p-0"
                            render={
                                <Link href={session.returnContext.returnTo} />
                            }
                        >
                            直接返回来源
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}
