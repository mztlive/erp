"use client"

import * as React from "react"
import { createPortal } from "react-dom"
import { HistoryIcon } from "lucide-react"
import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { InquiryHistory } from "./inquiry-history"
import type { ProcurementInquiry, SalesInquiry } from "../types"

type InquiryTab = "demand" | "feedback" | "published" | "fees"

/** 分区持续挂载；后台刷新只更新内容，不重置当前页签及输入。 */
export function InquiryDetailTabs({
    inquiry,
    procurement,
    demand,
    feedback,
    publishedFeedback,
    fees,
    actionsContainer,
    onFeedbackVisibilityChange,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    procurement: boolean
    demand: React.ReactNode
    feedback: React.ReactNode
    publishedFeedback?: React.ReactNode
    fees: React.ReactNode
    actionsContainer?: HTMLElement | null
    onFeedbackVisibilityChange?: (visible: boolean) => void
}) {
    const [active, setActive] = React.useState<InquiryTab>(() =>
        inquiry.status === "awaiting_sales" ||
        (procurement &&
            ["awaiting_procurement", "procurement_processing"].includes(
                inquiry.status,
            ))
            ? "feedback"
            : "demand",
    )
    const activeTab =
        active === "published" && !publishedFeedback ? "feedback" : active
    const tabs: InquiryTab[] = publishedFeedback
        ? ["demand", "feedback", "published", "fees"]
        : ["demand", "feedback", "fees"]
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
        published: [],
        fees: [],
    })
    React.useLayoutEffect(() => {
        onFeedbackVisibilityChange?.(activeTab === "feedback")
    }, [activeTab, onFeedbackVisibilityChange])
    React.useLayoutEffect(() => {
        for (const position of positions.current[activeTab]) {
            if (!position.element.isConnected) continue
            position.element.scrollTop = position.top
            position.element.scrollLeft = position.left
        }
    }, [activeTab])
    return (
        <Tabs
            value={activeTab}
            onValueChange={(value) => {
                if (
                    value !== "demand" &&
                    value !== "feedback" &&
                    value !== "published" &&
                    value !== "fees"
                )
                    return
                const panel = panels.current[activeTab]
                if (panel)
                    positions.current[activeTab] = [
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
            className="min-h-0 min-w-0 flex-1 gap-5 overflow-hidden"
        >
            <div className="flex shrink-0 flex-wrap items-center justify-between gap-x-6 gap-y-2 border-b">
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
                        ) : !publishedFeedback &&
                          inquiry.feedbacks.length > 0 ? (
                            <span className="text-xs text-muted-foreground">
                                {inquiry.feedbacks.length} 轮
                            </span>
                        ) : null}
                    </TabsTrigger>
                    {publishedFeedback && (
                        <TabsTrigger
                            id="sales-inquiry-tab-published"
                            value="published"
                        >
                            已发布反馈
                            <span className="text-xs text-muted-foreground">
                                {inquiry.feedbacks.length} 轮
                            </span>
                        </TabsTrigger>
                    )}
                    <TabsTrigger id="sales-inquiry-tab-fees" value="fees">
                        费用与办理
                    </TabsTrigger>
                </TabsList>
            </div>
            {actionsContainer &&
                createPortal(
                    <Button
                        id="sales-inquiry-history-open"
                        ref={historyTrigger}
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => setHistoryOpen(true)}
                    >
                        <HistoryIcon aria-hidden="true" />
                        操作记录
                    </Button>,
                    actionsContainer,
                )}
            {tabs.map((tab) => (
                <TabsContent
                    key={tab}
                    id={`sales-inquiry-panel-${tab}`}
                    value={tab}
                    keepMounted
                    hidden={activeTab !== tab}
                    ref={(element) => {
                        panels.current[tab] = element ?? undefined
                    }}
                    className="min-h-0 min-w-0 overflow-auto overscroll-contain"
                >
                    {tab === "demand"
                        ? demand
                        : tab === "feedback"
                          ? feedback
                          : tab === "published"
                            ? publishedFeedback
                            : fees}
                </TabsContent>
            ))}
            <QuickPreviewSheet
                idPrefix="sales-inquiry-history"
                open={historyOpen}
                onOpenChange={setHistoryOpen}
                finalFocus={historyTrigger}
                size="preview"
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
                <InquiryHistory inquiry={inquiry} />
            </QuickPreviewSheet>
        </Tabs>
    )
}
