"use client"

import Link from "next/link"

import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { SafetyPausePanel } from "@/features/product-publications/components/safety-pause-panel"
import { usePublicationDetailQuery } from "@/features/product-publications/hooks/queries"

export function PublicationPreviewSheet({
    previewId,
    onClose,
}: {
    previewId: string | null
    onClose: () => void
}) {
    const previewQuery = usePublicationDetailQuery(previewId)
    const previewRow = previewQuery.data

    return (
        <QuickPreviewSheet
            open={previewId != null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            title={previewRow?.selectedRevision.name ?? "发布预览"}
            description={
                previewRow
                    ? `${previewRow.identity.skuCode} · ${previewRow.identity.targetMallName}`
                    : undefined
            }
        >
            {previewQuery.isPending ? (
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            ) : previewRow ? (
                <div className="space-y-3 text-sm">
                    <dl className="grid gap-2 sm:grid-cols-2">
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                发布编号
                            </dt>
                            <dd className="num">
                                {previewRow.identity.publicationCode}
                            </dd>
                        </div>
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                发布状态
                            </dt>
                            <dd>{previewRow.statusLabel}</dd>
                        </div>
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                商城生效版
                            </dt>
                            <dd className="num">
                                {previewRow.currentAckedRevisionNo != null
                                    ? `r${previewRow.currentAckedRevisionNo}`
                                    : "尚未生效"}
                            </dd>
                        </div>
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                最新发布版
                            </dt>
                            <dd className="num">
                                {previewRow.latestRevisionNo != null
                                    ? `r${previewRow.latestRevisionNo}`
                                    : "—"}
                            </dd>
                        </div>
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                固定供给
                            </dt>
                            <dd>
                                {
                                    previewRow.selectedRevision.fixedOffering
                                        .supplierName
                                }
                                <div className="text-xs text-muted-foreground">
                                    {
                                        previewRow.selectedRevision
                                            .fixedOffering.availabilityLabel
                                    }
                                </div>
                            </dd>
                        </div>
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                商城接收
                            </dt>
                            <dd>
                                {(() => {
                                    const latestDelivery =
                                        previewRow.deliveries.find(
                                            (d) =>
                                                d.revisionId ===
                                                previewRow.latestRevisionId,
                                        )
                                    return latestDelivery?.statusLabel ?? "—"
                                })()}
                            </dd>
                        </div>
                    </dl>
                    {previewRow.safetyPause ? (
                        <>
                            <Separator />
                            <SafetyPausePanel
                                pause={previewRow.safetyPause}
                                compact
                                sourceObjectLabel={`${previewRow.selectedRevision.fixedOffering.supplierName} · ${previewRow.identity.skuCode}`}
                                affectedPublicationLabels={{
                                    [previewRow.identity.publicationId]:
                                        previewRow.identity.publicationCode,
                                }}
                            />
                        </>
                    ) : null}
                    <Button
                        type="button"
                        className="w-full"
                        render={
                            <Link
                                href={`/commerce/publications/${encodeURIComponent(previewRow.identity.publicationId)}`}
                            />
                        }
                    >
                        查看详情
                    </Button>
                </div>
            ) : null}
        </QuickPreviewSheet>
    )
}
