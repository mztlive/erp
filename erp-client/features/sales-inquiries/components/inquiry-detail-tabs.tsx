"use client"

import * as React from "react"
import { HistoryIcon } from "lucide-react"
import {
    DetailWorkspaceSection,
    DetailWorkspaceTabs,
    QuickPreviewSheet,
    detailWorkspaceStyles,
    type DetailWorkspaceTabItem,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import type { ProcurementInquiry, SalesInquiry } from "../types"
import { InquiryHistory } from "./inquiry-history"

type InquiryTab = "demand" | "feedback" | "published" | "fees"

/** 操作记录入口与抽屉：直接渲染于页头次要操作区，采用无框轻按钮语言。 */
export function InquiryHistoryButton({
    inquiry,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    workspace?: boolean
}) {
    const [historyOpen, setHistoryOpen] = React.useState(false)
    const historyTrigger = React.useRef<HTMLButtonElement>(null)
    return (
        <>
            <Button
                id="sales-inquiry-history-open"
                ref={historyTrigger}
                type="button"
                variant="ghost"
                className={detailWorkspaceStyles.quietButton}
                size="sm"
                onClick={() => setHistoryOpen(true)}
            >
                <HistoryIcon aria-hidden="true" data-icon="inline-start" />
                操作记录
            </Button>
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
        </>
    )
}

/** 分区持续挂载；后台刷新只更新内容，不重置当前页签及输入。 */
export function InquiryDetailTabs({
    inquiry,
    procurement,
    workspace,
    renderLayout,
    salesProducts,
    feedback,
    feedbackActions,
    publishedFeedback,
    fees,
    feeCount,
    onFeedbackVisibilityChange,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    procurement: boolean
    workspace: boolean
    renderLayout: (
        navigation: React.ReactNode,
        panels: React.ReactNode,
    ) => React.ReactNode
    salesProducts?: React.ReactNode
    feedback?: React.ReactNode
    feedbackActions?: React.ReactNode
    publishedFeedback?: React.ReactNode
    fees: React.ReactNode
    feeCount: number
    onFeedbackVisibilityChange?: (visible: boolean) => void
}) {
    const [active, setActive] = React.useState<InquiryTab>(() =>
        procurement ? "feedback" : "demand",
    )
    const activeTab: InquiryTab =
        (active === "published" && !publishedFeedback) ||
        (procurement && active === "demand")
            ? "feedback"
            : active
    const tabs: InquiryTab[] = !procurement
        ? ["demand", "fees"]
        : publishedFeedback
          ? ["feedback", "published", "fees"]
          : ["feedback", "fees"]

    React.useLayoutEffect(() => {
        onFeedbackVisibilityChange?.(activeTab === "feedback")
    }, [activeTab, onFeedbackVisibilityChange])

    const tabItems: DetailWorkspaceTabItem[] = []
    if (!procurement) {
        tabItems.push({
            id: "demand",
            triggerId: "sales-inquiry-tab-demand",
            label: "商品与报价",
            badge:
                inquiry.status === "awaiting_sales" ? (
                    <span className="text-xs text-muted-foreground">
                        待确认
                    </span>
                ) : undefined,
        })
    } else {
        tabItems.push({
            id: "feedback",
            triggerId: "sales-inquiry-tab-feedback",
            label: "采购反馈",
            badge:
                inquiry.status === "awaiting_sales" ? (
                    <span className="text-xs text-muted-foreground">
                        待确认
                    </span>
                ) : !publishedFeedback && inquiry.feedbacks.length > 0 ? (
                    <span className="num text-xs text-muted-foreground">
                        {inquiry.feedbacks.length} 轮
                    </span>
                ) : undefined,
        })
        if (publishedFeedback) {
            tabItems.push({
                id: "published",
                triggerId: "sales-inquiry-tab-published",
                label: "已发布反馈",
                badge: (
                    <span className="num text-xs text-muted-foreground">
                        {inquiry.feedbacks.length} 轮
                    </span>
                ),
            })
        }
    }
    tabItems.push({
        id: "fees",
        triggerId: "sales-inquiry-tab-fees",
        label: "费用与办理",
        badge:
            feeCount > 0 ? (
                <StatusBadge tone="warning" label={`额外费用 ${feeCount} 项`} />
            ) : undefined,
    })

    const handleTabChange = (next: string) => {
        if (
            next === "demand" ||
            next === "feedback" ||
            next === "published" ||
            next === "fees"
        ) {
            setActive(next)
        }
    }

    const navigation = (
        <DetailWorkspaceTabs
            value={activeTab}
            onValueChange={handleTabChange}
            items={tabItems}
            ariaLabel="询价内容分区"
        />
    )

    const panels = tabs.map((tab) => {
        const content =
            tab === "demand"
                ? salesProducts
                : tab === "feedback"
                  ? feedback
                  : tab === "published"
                    ? publishedFeedback
                    : fees
        const isActive = activeTab === tab
        return (
            <div
                key={tab}
                id={`sales-inquiry-panel-${tab}`}
                role="tabpanel"
                aria-labelledby={`sales-inquiry-tab-${tab}`}
                hidden={!isActive}
                className={workspace ? "min-w-0" : "min-w-0 py-4"}
            >
                {workspace ? (
                    <DetailWorkspaceSection
                        divider={false}
                        className="py-5 first:pt-4 last:pb-10"
                        actions={
                            tab === "feedback" ? feedbackActions : undefined
                        }
                    >
                        {content}
                    </DetailWorkspaceSection>
                ) : (
                    content
                )}
            </div>
        )
    })

    return <>{renderLayout(navigation, panels)}</>
}
