"use client"

import { ArrowUpRightIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import type { SettlementListRow } from "@/features/supplier-settlements/types"

export function SettlementListPreviewSheet({
    open,
    row,
    onOpenChange,
    onOpen,
    patchUrl,
}: {
    open: boolean
    row: SettlementListRow | null
    onOpenChange: (open: boolean) => void
    onOpen: (statementId: string) => void
    patchUrl: (patch: Partial<SettlementsUrlState>) => void
}) {
    return (
        <QuickPreviewSheet
            id="supplier-settlements-list-preview-sheet"
            open={open}
            onOpenChange={onOpenChange}
            size="detail"
            title={row?.supplierName ?? "结算预览"}
            identity={
                row ? (
                    <span className="num">结算单号：{row.statementNo}</span>
                ) : null
            }
            description={row?.periodLabel}
            summary={
                row ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={row.statusLabel}
                        tone={row.statusTone}
                    />
                ) : null
            }
            footer={
                <>
                    <Button
                        id="supplier-settlements-list-preview-close"
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        关闭
                    </Button>
                    {row?.unresolvedDifferenceCount ? (
                        <Button
                            id="supplier-settlements-list-preview-open-differences"
                            type="button"
                            variant="outline"
                            onClick={() =>
                                patchUrl({
                                    statementId: row.statementId,
                                    section: "differences",
                                    preview: undefined,
                                })
                            }
                        >
                            打开差异处理
                        </Button>
                    ) : null}
                    {row ? (
                        <Button
                            id="supplier-settlements-list-preview-open"
                            type="button"
                            onClick={() => onOpen(row.statementId)}
                        >
                            打开结算单
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                aria-hidden
                            />
                        </Button>
                    ) : null}
                </>
            }
        >
            <div className="min-h-0 flex-1 space-y-6 overflow-y-auto px-7 py-6 text-sm">
                {row ? (
                    <>
                        <section className="space-y-3 border-b border-border pb-6">
                            <h3 className="text-xs font-medium text-muted-foreground">
                                ERP 计算金额（含税）
                            </h3>
                            <MoneyValue
                                value={row.erpAmountGross}
                                className="[&>span:first-child]:text-[32px] [&>span:first-child]:font-semibold [&>span:first-child]:tracking-tight"
                            />
                            <dl className="space-y-3 pt-3">
                                <div className="flex items-baseline justify-between gap-5">
                                    <dt className="text-xs text-muted-foreground">
                                        供应商账单金额（含税）
                                    </dt>
                                    <dd>
                                        {row.supplierAmountGross ? (
                                            <MoneyValue
                                                value={row.supplierAmountGross}
                                            />
                                        ) : (
                                            "账单未同步"
                                        )}
                                    </dd>
                                </div>
                                <div className="flex items-baseline justify-between gap-5">
                                    <dt className="text-xs text-muted-foreground">
                                        差异
                                    </dt>
                                    <dd className="text-right">
                                        {row.differenceAmountGross ? (
                                            <MoneyValue
                                                value={
                                                    row.differenceAmountGross
                                                }
                                            />
                                        ) : (
                                            "—"
                                        )}
                                        {row.differenceDirectionLabel ? (
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                {row.differenceDirectionLabel}
                                            </p>
                                        ) : null}
                                    </dd>
                                </div>
                            </dl>
                        </section>
                        <section className="space-y-3">
                            <h3 className="font-medium">经办与复核</h3>
                            <dl className="space-y-3">
                                <div className="flex items-baseline justify-between gap-5">
                                    <dt className="text-xs text-muted-foreground">
                                        经办
                                    </dt>
                                    <dd className="min-w-0 break-words text-right text-[13px]">
                                        {row.preparedByLabel || "—"}
                                    </dd>
                                </div>
                                <div className="flex items-baseline justify-between gap-5">
                                    <dt className="text-xs text-muted-foreground">
                                        复核
                                    </dt>
                                    <dd className="min-w-0 break-words text-right text-[13px]">
                                        {row.reviewedByLabel || "—"}
                                    </dd>
                                </div>
                            </dl>
                        </section>
                        <p className="border-t border-border pt-6 text-xs leading-5 text-muted-foreground">
                            详情页可继续提交复核并查询处理结果。
                        </p>
                    </>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        未找到预览行，可能已被移出当前筛选范围。
                    </p>
                )}
            </div>
        </QuickPreviewSheet>
    )
}
