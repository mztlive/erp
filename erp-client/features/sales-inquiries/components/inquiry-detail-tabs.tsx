"use client"

import * as React from "react"
import { HistoryIcon } from "lucide-react"
import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { InquiryHistory } from "./inquiry-history"
import type { ProcurementInquiry, SalesInquiry } from "../types"

type InquiryTab = "demand" | "feedback" | "fees"

/** 分区持续挂载；后台刷新只更新内容，不重置当前页签及输入。 */
export function InquiryDetailTabs({
    inquiry,
    procurement,
    demand,
    feedback,
    fees,
    actions,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    procurement: boolean
    demand: React.ReactNode
    feedback: React.ReactNode
    fees: React.ReactNode
    actions?: React.ReactNode
}) {
    const [active, setActive] = React.useState<InquiryTab>(() =>
        inquiry.status === "awaiting_sales" ||
        (procurement && inquiry.status === "procurement_processing")
            ? "feedback"
            : "demand",
    )
    const [historyOpen, setHistoryOpen] = React.useState(false)
    const historyTrigger = React.useRef<HTMLButtonElement>(null)
    const panels = React.useRef<Partial<Record<InquiryTab, HTMLDivElement>>>({})
    const positions = React.useRef<
        Record<
            InquiryTab,
            { element: HTMLElement; top: number; left: number }[]
        >
    >({
        demand: [],
        feedback: [],
        fees: [],
    })
    React.useLayoutEffect(() => {
        for (const position of positions.current[active]) {
            if (!position.element.isConnected) continue
            position.element.scrollTop = position.top
            position.element.scrollLeft = position.left
        }
    }, [active])
    return (
        <Tabs
            value={active}
            onValueChange={(value) => {
                if (
                    value !== "demand" &&
                    value !== "feedback" &&
                    value !== "fees"
                )
                    return
                const panel = panels.current[active]
                if (panel)
                    positions.current[active] = [
                        panel,
                        ...panel.querySelectorAll<HTMLElement>(
                            '[data-slot="table-container"]',
                        ),
                    ].map((element) => ({
                        element,
                        top: element.scrollTop,
                        left: element.scrollLeft,
                    }))
                setActive(value)
            }}
            className="min-w-0 gap-5"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-b">
                <TabsList
                    variant="line"
                    aria-label="询价内容分区"
                    className="max-w-full flex-wrap gap-x-5"
                >
                    <TabsTrigger id="sales-inquiry-tab-demand" value="demand">
                        商品需求
                    </TabsTrigger>
                    <TabsTrigger
                        id="sales-inquiry-tab-feedback"
                        value="feedback"
                    >
                        采购反馈
                        {inquiry.status === "awaiting_sales" ? (
                            <span className="text-xs text-muted-foreground">
                                待确认
                            </span>
                        ) : inquiry.feedbacks.length > 0 ? (
                            <span className="text-xs text-muted-foreground">
                                {inquiry.feedbacks.length} 轮
                            </span>
                        ) : null}
                    </TabsTrigger>
                    <TabsTrigger id="sales-inquiry-tab-fees" value="fees">
                        费用与办理
                    </TabsTrigger>
                </TabsList>
                <div className="ml-auto flex max-w-full flex-wrap items-center justify-end gap-2 py-1">
                    <Button
                        id="sales-inquiry-history-open"
                        ref={historyTrigger}
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => setHistoryOpen(true)}
                    >
                        <HistoryIcon aria-hidden="true" />
                        操作记录
                    </Button>
                    {actions}
                </div>
            </div>
            {(["demand", "feedback", "fees"] as const).map((tab) => (
                <TabsContent
                    key={tab}
                    id={`sales-inquiry-panel-${tab}`}
                    value={tab}
                    keepMounted
                    hidden={active !== tab}
                    ref={(element) => {
                        panels.current[tab] = element ?? undefined
                    }}
                    className="min-w-0 max-h-[max(24rem,calc(100dvh-20rem))] overflow-auto overscroll-contain"
                >
                    {tab === "demand"
                        ? demand
                        : tab === "feedback"
                          ? feedback
                          : fees}
                </TabsContent>
            ))}
            <QuickPreviewSheet
                idPrefix="sales-inquiry-history"
                open={historyOpen}
                onOpenChange={setHistoryOpen}
                finalFocus={historyTrigger}
                size="detail"
                title="操作记录"
                identity={`询价单号：${inquiry.inquiry_no}`}
                description={inquiry.customer_name}
                footer={
                    <Button
                        id="sales-inquiry-history-dismiss"
                        type="button"
                        variant="outline"
                        onClick={() => setHistoryOpen(false)}
                    >
                        关闭
                    </Button>
                }
            >
                <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
                    <InquiryHistory inquiry={inquiry} />
                </div>
            </QuickPreviewSheet>
        </Tabs>
    )
}
