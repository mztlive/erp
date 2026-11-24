"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { regionFullName, type RegionIndex } from "@/features/regions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed } from "@/lib/fixed-decimal"
import type { DeliveryImportPreview } from "../lib/sales-delivery-import"
import type { SalesDeliveryAllocation } from "../types"

function addressText(
    address: SalesDeliveryAllocation,
    regions: RegionIndex,
    unit: string,
) {
    const region = address.region_code
        ? regionFullName(regions, address.region_code)
        : ""
    return `${address.recipient} · ${address.phone} · ${region}${address.address} · ${compactFixed(address.quantity)} ${unit}`
}

export function SalesDeliveryImportPreview({
    result,
    regions,
    filename,
    prefix,
    onDownloadIssues,
}: {
    result: DeliveryImportPreview
    regions: RegionIndex
    filename: string
    prefix: string
    onDownloadIssues: () => void
}) {
    const [expanded, setExpanded] = useState("")
    const changed = result.groups.filter((group) => group.changes.length)
    return (
        <div className="space-y-3">
            <p role="status" className="break-words text-body-compact">
                {filename} · {changed.length} 项商品有地址变更
                {result.skipped ? ` · 跳过 ${result.skipped} 行重复地址` : ""}
                {result.issues.length
                    ? ` · ${result.issues.length} 条问题`
                    : ""}
            </p>
            {result.issues.length > 0 && (
                <div className="space-y-2 rounded-md border p-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <p className="text-body-compact font-medium">
                            修改问题行后重新导入
                        </p>
                        <Button
                            id={`${prefix}-download-issues`}
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onDownloadIssues}
                        >
                            下载问题行
                        </Button>
                    </div>
                    <ul className="space-y-1 text-body-compact text-destructive">
                        {result.issues.slice(0, 25).map((issue, index) => (
                            <li
                                key={`${issue.row}-${index}`}
                                className="break-words"
                            >
                                第 {issue.row} 行
                                {issue.label ? ` · ${issue.label}` : ""}：
                                {issue.message}
                            </li>
                        ))}
                    </ul>
                    {result.issues.length > 25 && (
                        <p className="text-xs text-muted-foreground">
                            下载问题行可查看全部 {result.issues.length} 条。
                        </p>
                    )}
                </div>
            )}
            {result.groups.length > 0 && (
                <ul className="divide-y rounded-md border">
                    {result.groups.map((group) => {
                        const id = `${prefix}-${toAutomationIdSegment(group.line.rowKey)}`
                        const isExpanded = expanded === group.line.rowKey
                        return (
                            <li
                                key={group.line.rowKey}
                                className="space-y-2 px-3 py-2"
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div className="min-w-0 break-words text-body-compact">
                                        <p className="font-medium">
                                            {group.label}
                                        </p>
                                        <p
                                            className={
                                                group.summary.complete
                                                    ? "text-muted-foreground"
                                                    : "text-destructive"
                                            }
                                        >
                                            {group.summary.summary}
                                            {!group.summary.complete &&
                                            group.summary.status
                                                ? ` · ${group.summary.status}`
                                                : ""}
                                        </p>
                                    </div>
                                    {group.changes.length > 0 &&
                                        group.allocations.length <= 100 && (
                                            <Button
                                                id={`${id}-toggle`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                aria-expanded={isExpanded}
                                                aria-controls={`${id}-changes`}
                                                onClick={() =>
                                                    setExpanded(
                                                        isExpanded
                                                            ? ""
                                                            : group.line.rowKey,
                                                    )
                                                }
                                            >
                                                {isExpanded
                                                    ? "收起变更"
                                                    : `查看 ${group.changes.length} 项变更`}
                                            </Button>
                                        )}
                                </div>
                                <div id={`${id}-changes`} hidden={!isExpanded}>
                                    {isExpanded && (
                                        <ul className="space-y-2 rounded-md bg-muted/30 p-3 text-body-compact">
                                            {group.changes.map(
                                                ({ before, after }, index) => (
                                                    <li
                                                        key={index}
                                                        className="break-words"
                                                    >
                                                        <p className="font-medium">
                                                            {before && after
                                                                ? "修改"
                                                                : before
                                                                  ? "移除"
                                                                  : "新增"}
                                                        </p>
                                                        {before && (
                                                            <p className="text-muted-foreground">
                                                                {after
                                                                    ? "原地址 · "
                                                                    : ""}
                                                                {addressText(
                                                                    before,
                                                                    regions,
                                                                    group.unitName,
                                                                )}
                                                            </p>
                                                        )}
                                                        {after && (
                                                            <p>
                                                                {before
                                                                    ? "导入后 · "
                                                                    : ""}
                                                                {addressText(
                                                                    after,
                                                                    regions,
                                                                    group.unitName,
                                                                )}
                                                            </p>
                                                        )}
                                                    </li>
                                                ),
                                            )}
                                        </ul>
                                    )}
                                </div>
                            </li>
                        )
                    })}
                </ul>
            )}
        </div>
    )
}
