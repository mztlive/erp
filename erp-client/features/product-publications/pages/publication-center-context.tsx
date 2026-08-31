"use client"

import {
    FormalActionResult,
    StatusTrackSummary,
    surfacePanelClassName,
} from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"

import { PublishGateAlert } from "@/features/product-publications/components/publish-gate-alert"
import { SafetyPausePanel } from "@/features/product-publications/components/safety-pause-panel"
import type { ProductPublicationView } from "@/features/product-publications/types"
import { SECTIONS, type SectionId } from "./publication-center-navigation"

/** 一屏识别：稳定发布 / 商城生效版 / 最新待确认版 */
export function PublicationCenterVersionSummary({
    data,
}: {
    data: ProductPublicationView
}) {
    const ackedLabel =
        data.currentAckedRevisionNo != null
            ? `r${data.currentAckedRevisionNo}`
            : "尚未生效"
    const latestLabel =
        data.latestRevisionNo != null ? `r${data.latestRevisionNo}` : "—"

    return (
        <Card className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid pb-2">
                <CardTitle className="text-base">发布身份与版本</CardTitle>
                <CardDescription>
                    展示商城实际生效版本与最新提交版本，避免误判。
                </CardDescription>
            </CardHeader>
            <CardContent>
                <StatusTrackSummary
                    variant="table"
                    tracks={[
                        {
                            id: "stable",
                            label: "稳定发布",
                            status: {
                                label: data.identity.publicationCode,
                                tone: "neutral",
                                description: `${data.identity.skuCode} · ${data.identity.targetMallName}`,
                            },
                        },
                        {
                            id: "acked",
                            label: "当前商城生效版",
                            status: {
                                label: ackedLabel,
                                tone:
                                    data.currentAckedRevisionNo != null
                                        ? "success"
                                        : "warning",
                                description:
                                    data.currentAckedRevisionNo != null
                                        ? "商城已成功确认"
                                        : "商城确认前不显示已生效",
                            },
                        },
                        {
                            id: "latest",
                            label: "最新发布版",
                            status: {
                                label: latestLabel,
                                tone:
                                    data.latestRevisionNo ===
                                    data.currentAckedRevisionNo
                                        ? "success"
                                        : "info",
                                description:
                                    data.currentAckedRevisionNo != null &&
                                    data.latestRevisionNo !==
                                        data.currentAckedRevisionNo
                                        ? "等待商城确认"
                                        : "与商城生效版一致或尚无待确认",
                            },
                        },
                    ]}
                />
                {/* hasPendingConfirmation is on row not view — derive */}
                {data.currentAckedRevisionNo != null &&
                data.latestRevisionNo !== data.currentAckedRevisionNo ? (
                    <p className="mt-2 text-xs text-muted-foreground">
                        最新 r{data.latestRevisionNo}{" "}
                        尚未被商城确认，商城生效前仍按待确认处理。
                    </p>
                ) : null}
            </CardContent>
        </Card>
    )
}

/**
 * 页头与内容区之间的上下文条：未保存提示、最近结果、版本摘要、
 * 锚点导航、安全暂停面板与发布门禁。
 */
export function PublicationCenterContextBar({
    data,
    dirty,
    onDiscard,
    lastResult,
    section,
    onSectionChange,
}: {
    data: ProductPublicationView
    dirty: boolean
    onDiscard: () => void
    lastResult: ResultState
    section: SectionId
    onSectionChange: (section: SectionId) => void
}) {
    return (
        <>
            {dirty ? (
                <Alert variant="warning" role="status">
                    <AlertTitle>本次编辑 · 未保存</AlertTitle>
                    <AlertDescription>
                        当前输入仅保存在当前页面，无草稿保存、无自动保存。刷新或关闭前将提示丢失。
                        <div className="mt-2 flex gap-2">
                            <Button
                                id="publication-center-context-discard"
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={onDiscard}
                            >
                                放弃输入
                            </Button>
                        </div>
                    </AlertDescription>
                </Alert>
            ) : null}

            {lastResult ? (
                <FormalActionResult
                    status={
                        lastResult.status === "failed"
                            ? "blocked"
                            : lastResult.status
                    }
                    title={lastResult.title}
                    description={lastResult.description}
                    reference={lastResult.reference}
                    facts={lastResult.facts}
                />
            ) : null}

            <PublicationCenterVersionSummary data={data} />

            <nav
                role="group"
                aria-label="发布对象锚点"
                className="sticky top-0 z-10 inline-flex flex-wrap rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
            >
                {SECTIONS.map((s) => (
                    <button
                        key={s.id}
                        id={`publication-center-nav-${toAutomationIdSegment(s.id)}`}
                        type="button"
                        aria-pressed={section === s.id}
                        onClick={() => onSectionChange(s.id)}
                        className={cn(
                            "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all",
                            section === s.id
                                ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                        )}
                    >
                        {s.label}
                    </button>
                ))}
            </nav>

            {data.safetyPause ? (
                <div id="pub-section-overview-safety">
                    <SafetyPausePanel
                        pause={data.safetyPause}
                        sourceObjectLabel={`${data.selectedRevision.fixedOffering.supplierName} · ${data.identity.skuCode}`}
                        affectedPublicationLabels={{
                            [data.identity.publicationId]:
                                data.identity.publicationCode,
                        }}
                    />
                </div>
            ) : null}

            <PublishGateAlert gate={data.publishGate} />
        </>
    )
}
