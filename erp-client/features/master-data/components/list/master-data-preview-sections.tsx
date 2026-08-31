"use client"

import Link from "next/link"
import { ChevronDownIcon } from "lucide-react"

import { RevisionTimeline, SensitiveValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { revealMasterDataSensitive } from "@/features/master-data/api"
import {
    masterDataActionLabel,
    masterDataCopy,
} from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"

type PreviewSensitiveField = Readonly<{
    label: string
    maskedValue: string
    revealToken?: string
    visibility: "full" | "masked" | "hidden"
}>

/** 预览 · 敏感信息（已打码，可短时查看）。 */
export function PreviewSensitiveSection({
    fields,
    canRevealSensitive,
}: {
    fields: readonly PreviewSensitiveField[]
    canRevealSensitive: boolean
}) {
    return (
        <>
            <Separator />
            <section className="space-y-2">
                <h3 className="text-xs font-medium text-muted-foreground">
                    {masterDataCopy.previewSensitive}
                </h3>
                <ul className="space-y-2">
                    {fields.map((field) => (
                        <li
                            key={field.label}
                            className="flex flex-wrap items-center gap-2"
                        >
                            <span className="text-muted-foreground">
                                {field.label}
                            </span>
                            {field.visibility === "masked" &&
                            field.revealToken &&
                            canRevealSensitive ? (
                                <SensitiveValue
                                    label={field.label}
                                    maskedValue={field.maskedValue}
                                    onReveal={() =>
                                        revealMasterDataSensitive(
                                            field.revealToken!,
                                        )
                                    }
                                />
                            ) : (
                                <code className="num rounded bg-muted px-2 py-0.5 text-xs">
                                    {field.maskedValue}
                                </code>
                            )}
                        </li>
                    ))}
                </ul>
            </section>
        </>
    )
}

/** 预览 · 库存摘要（只读）。 */
export function PreviewStockSection({
    policyNote,
    onHandQty,
    reservedQty,
    w10Href,
}: {
    policyNote: string
    onHandQty: string
    reservedQty: string
    w10Href: string
}) {
    return (
        <>
            <Separator />
            <section className="space-y-2">
                <h3 className="text-xs font-medium text-muted-foreground">
                    {masterDataCopy.previewStock}
                </h3>
                <p className="text-xs text-muted-foreground">{policyNote}</p>
                <p>
                    在库 <span className="num">{onHandQty}</span>
                    {" · "}
                    预占 <span className="num">{reservedQty}</span>
                </p>
                <Button
                    id="master-data-list-master-data-preview-sections-button-1"
                    type="button"
                    size="sm"
                    variant="outline"
                    render={<Link href={w10Href} />}
                >
                    打开库存台账
                </Button>
            </section>
        </>
    )
}

/** 预览 · 资料变更历史（折叠）。 */
export function PreviewHistorySection({
    revisions,
}: {
    revisions: readonly {
        id: string
        revisionNo: number
        timingLabel: string
        nameSnapshot: string
        actor: string
        effectiveFrom: string
        effectiveTo?: string
        changeReason: string
        isCurrent: boolean
        lifecycleAtRevision: "ENABLED" | "DISABLED"
    }[]
}) {
    return (
        <>
            <Separator />
            <section className="space-y-2">
                <details className="group" open={false}>
                    <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-muted-foreground [&::-webkit-details-marker]:hidden">
                        {masterDataCopy.previewHistory}
                        <ChevronDownIcon
                            className="size-3.5 transition-transform group-open:rotate-180"
                            aria-hidden
                        />
                    </summary>
                    <div className="mt-2">
                        <RevisionTimeline
                            revisions={revisions.map((rev) => ({
                                id: rev.id,
                                version: rev.revisionNo,
                                source: "erp-change" as const,
                                actor: rev.actor,
                                effectiveAt: {
                                    dateTime: rev.effectiveFrom,
                                    label: formatEffectiveRange(
                                        rev.effectiveFrom,
                                        rev.effectiveTo,
                                    ),
                                },
                                reason: (
                                    <div className="space-y-1">
                                        <div>
                                            {masterDataCopy.centerHistoryName}：
                                            <strong>{rev.nameSnapshot}</strong>
                                        </div>
                                        <div className="text-muted-foreground">
                                            {rev.changeReason}
                                        </div>
                                        <div className="flex flex-wrap gap-2">
                                            <Badge variant="outline">
                                                {rev.timingLabel}
                                            </Badge>
                                            <Badge variant="secondary">
                                                {rev.lifecycleAtRevision ===
                                                "ENABLED"
                                                    ? "启用"
                                                    : "停用"}
                                            </Badge>
                                        </div>
                                    </div>
                                ),
                                isCurrent: rev.isCurrent,
                            }))}
                        />
                    </div>
                </details>
            </section>
        </>
    )
}

/** 预览 · 当前无法进行的操作（折叠）。 */
export function PreviewBlockedSection({
    blockers,
}: {
    blockers: readonly { action: string; code: string; message: string }[]
}) {
    return (
        <>
            <Separator />
            <section className="space-y-2">
                <details className="group">
                    <summary className="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-muted-foreground [&::-webkit-details-marker]:hidden">
                        {masterDataCopy.previewActionBlocked}
                        <ChevronDownIcon
                            className="size-3.5 transition-transform group-open:rotate-180"
                            aria-hidden
                        />
                    </summary>
                    <ul className="mt-2 space-y-1 text-xs">
                        {blockers.map((b) => (
                            <li key={`${b.action}-${b.code}`}>
                                <span className="font-medium">
                                    {masterDataActionLabel(b.action)}
                                </span>
                                <div className="text-muted-foreground">
                                    {b.message}
                                </div>
                            </li>
                        ))}
                    </ul>
                </details>
            </section>
        </>
    )
}
