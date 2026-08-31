"use client"

import Link from "next/link"
import { LoaderCircleIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    DocumentSection,
    RevisionTimeline,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { formatDateTime } from "@/lib/datetime"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { goToWorkspaceLabel } from "@/lib/ui-text"
import { cn } from "@/lib/utils"
import { MEDIA_ROLE_LABEL } from "@/features/product-publications/types"
import type { ProductPublicationView } from "@/features/product-publications/types"

export function PublicationCenterContentSections({
    data,
    isViewingHistoricalRevision,
    onClearRevision,
    onSelectRevision,
    canRetryDelivery,
    retryPending,
    onRetryDelivery,
}: {
    data: ProductPublicationView
    isViewingHistoricalRevision: boolean
    onClearRevision: () => void
    onSelectRevision: (revisionId: string) => void
    canRetryDelivery: boolean
    retryPending: boolean
    onRetryDelivery: (deliveryId: string) => void
}) {
    const ackedLabel =
        data.currentAckedRevisionNo != null
            ? `r${data.currentAckedRevisionNo}`
            : "尚未生效"
    const latestLabel =
        data.latestRevisionNo != null ? `r${data.latestRevisionNo}` : "—"

    return (
        <>
            <DocumentSection
                id="pub-section-media"
                title="媒体"
                description="主图、轮播、详情图及替代文本"
            >
                <ul className="grid gap-2 sm:grid-cols-3">
                    {data.selectedRevision.media.map((m) => (
                        <li
                            key={`${m.fileAssetId}-${m.mediaRole}-${m.sortNo}`}
                            className="rounded-lg bg-muted/40 p-3 text-sm"
                        >
                            <div className="mb-2 flex size-full min-h-20 items-center justify-center rounded bg-muted text-xs text-muted-foreground">
                                {MEDIA_ROLE_LABEL[m.mediaRole]}
                            </div>
                            <div className="font-medium">
                                {MEDIA_ROLE_LABEL[m.mediaRole]}
                            </div>
                            <div className="text-xs text-muted-foreground">
                                {m.altText}
                            </div>
                        </li>
                    ))}
                </ul>
            </DocumentSection>

            <DocumentSection
                id="pub-section-offering"
                title="固定供给"
                description="本版本唯一履约来源"
            >
                <Card className="border-0 bg-muted/40 shadow-none ring-0">
                    <CardContent className="space-y-2 pt-4 text-sm">
                        <div>
                            供应商{" "}
                            {data.selectedRevision.fixedOffering.supplierName}
                        </div>
                        <div>
                            可供状态{" "}
                            {
                                data.selectedRevision.fixedOffering
                                    .availabilityLabel
                            }
                        </div>
                        <div>
                            供货价{" "}
                            {data.fieldPermissions.supplyPriceGross ===
                                "masked" ||
                            !data.selectedRevision.fixedOffering
                                .supplyPriceVisible
                                ? "******"
                                : data.selectedRevision.fixedOffering
                                        .supplyPriceGross
                                  ? `¥${data.selectedRevision.fixedOffering.supplyPriceGross}`
                                  : "—"}
                        </div>
                        <p className="text-xs text-muted-foreground">
                            每次发布对应一个固定供给版本；修改图片、供给、价格或销售状态都会形成新版本，不覆盖历史。
                        </p>
                    </CardContent>
                </Card>
            </DocumentSection>

            <DocumentSection
                id="pub-section-delivery"
                title="发送与版本"
                description="各版本发送与商城确认时间线"
                action={
                    isViewingHistoricalRevision ? (
                        <Button
                            id="publication-center-sections-clear-revision"
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={onClearRevision}
                        >
                            回到最新版本
                        </Button>
                    ) : undefined
                }
            >
                <RevisionTimeline
                    revisions={data.revisions
                        .slice()
                        .reverse()
                        .map((r) => ({
                            id: r.revisionId,
                            version: r.revisionNo,
                            source: "erp-change" as const,
                            actor: r.createdBy,
                            effectiveAt: {
                                dateTime: r.createdAt,
                                label: formatDateTime(r.createdAt, "default"),
                            },
                            reason: r.deliverySummary,
                            status: {
                                label: r.saleStatusLabel,
                                tone: r.isMallAcked
                                    ? ("success" as const)
                                    : r.isLatest
                                      ? ("info" as const)
                                      : ("neutral" as const),
                            },
                            isCurrent:
                                r.revisionId ===
                                data.selectedRevision.revisionId,
                            action: (
                                <Button
                                    id={`publication-center-sections-revision-${toAutomationIdSegment(r.revisionId)}-view`}
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    onClick={() =>
                                        onSelectRevision(r.revisionId)
                                    }
                                >
                                    查看历史记录
                                </Button>
                            ),
                        }))}
                />
                <Separator className="my-4" />
                <div className="space-y-2">
                    <div className="text-sm font-medium">发送记录</div>
                    {data.deliveries.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            暂无发送
                        </p>
                    ) : (
                        <ul className="space-y-2">
                            {data.deliveries
                                .slice()
                                .reverse()
                                .map((d) => (
                                    <li
                                        key={d.deliveryId}
                                        className={cn(
                                            "rounded-lg p-3 text-sm ring-1",
                                            d.revisionId ===
                                                data.selectedRevision.revisionId
                                                ? "bg-primary/5 ring-primary/40"
                                                : "bg-muted/40 ring-transparent",
                                        )}
                                    >
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <div>
                                                <span className="num font-medium">
                                                    {d.deliveryId}
                                                </span>
                                                <span className="mx-2 text-muted-foreground">
                                                    r{d.revisionNo}
                                                </span>
                                                <BusinessStatusBadge
                                                    context="list"
                                                    label={d.statusLabel}
                                                    tone={d.statusTone}
                                                />
                                            </div>
                                            {d.status === "FAILED" ? (
                                                <Button
                                                    id={`publication-center-sections-delivery-${toAutomationIdSegment(d.deliveryId)}-retry`}
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    disabled={
                                                        !canRetryDelivery ||
                                                        retryPending
                                                    }
                                                    onClick={() =>
                                                        onRetryDelivery(
                                                            d.deliveryId,
                                                        )
                                                    }
                                                >
                                                    {retryPending ? (
                                                        <LoaderCircleIcon className="animate-spin" />
                                                    ) : null}
                                                    重试发送
                                                </Button>
                                            ) : null}
                                            {d.status === "HANDOFF" ? (
                                                <Button
                                                    id={`publication-center-sections-delivery-${toAutomationIdSegment(d.deliveryId)}-workspace`}
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    render={
                                                        <Link
                                                            href={`/governance/integration-errors?q=${encodeURIComponent(d.deliveryId)}`}
                                                        />
                                                    }
                                                >
                                                    {goToWorkspaceLabel("W29")}
                                                </Button>
                                            ) : null}
                                        </div>
                                        <div className="mt-1 text-xs text-muted-foreground">
                                            尝试 {d.attemptCount}
                                            {d.lastAttemptAt
                                                ? ` · 最近 ${formatDateTime(d.lastAttemptAt, "default")}`
                                                : ""}
                                            {d.mallAckAt
                                                ? ` · 商城确认 ${formatDateTime(d.mallAckAt, "default")}`
                                                : ""}
                                            {d.mallVersion ? (
                                                <>
                                                    {" · 商城版本 "}
                                                    <span className="num">
                                                        {d.mallVersion}
                                                    </span>
                                                </>
                                            ) : null}
                                        </div>
                                        {d.errorSummary ? (
                                            <p className="mt-1 text-xs text-destructive">
                                                {d.errorSummary}
                                            </p>
                                        ) : null}
                                    </li>
                                ))}
                        </ul>
                    )}
                </div>
            </DocumentSection>

            <DocumentSection
                id="pub-section-audit"
                title="审计"
                description="创建、提交、暂停与处理记录摘要"
            >
                <ul className="space-y-2 text-sm">
                    {data.revisions
                        .slice()
                        .reverse()
                        .map((r) => (
                            <li
                                key={r.revisionId}
                                className="flex flex-wrap justify-between gap-2 border-b border-grid py-2"
                            >
                                <span>
                                    r{r.revisionNo} · {r.createdBy} ·{" "}
                                    {r.saleStatusLabel}
                                </span>
                                <span className="num text-xs text-muted-foreground">
                                    {formatDateTime(r.createdAt, "default")}
                                </span>
                            </li>
                        ))}
                </ul>
            </DocumentSection>

            <aside className="min-w-0 space-y-3 xl:sticky xl:top-14 xl:self-start">
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-grid pb-2">
                        <CardTitle className="text-sm">选中修订</CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-1 text-sm">
                        <div className="num font-medium">
                            r{data.selectedRevision.revisionNo}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {data.selectedRevision.createdBy} ·{" "}
                            {formatDateTime(
                                data.selectedRevision.createdAt,
                                "default",
                            )}
                        </div>
                        <BusinessStatusBadge
                            context="preview"
                            label={data.selectedRevision.saleStatusLabel}
                            tone="neutral"
                        />
                        <div className="pt-2 text-xs">
                            供给{" "}
                            {data.selectedRevision.fixedOffering.supplierName}
                        </div>
                    </CardContent>
                </Card>
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-grid pb-2">
                        <CardTitle className="text-sm">版本对照</CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-2 text-sm">
                        <div className="flex justify-between">
                            <span className="text-muted-foreground">
                                商城生效
                            </span>
                            <span className="num">{ackedLabel}</span>
                        </div>
                        <div className="flex justify-between">
                            <span className="text-muted-foreground">
                                最新发布
                            </span>
                            <span className="num">{latestLabel}</span>
                        </div>
                    </CardContent>
                </Card>
            </aside>
        </>
    )
}
