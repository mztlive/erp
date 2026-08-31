"use client"

import {
    BusinessStatusBadge,
    DocumentTotals,
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
            title={row?.statementNo ?? "结算预览"}
            description={
                row ? `${row.supplierName} · ${row.periodLabel}` : undefined
            }
        >
            {row ? (
                <div className="space-y-4 p-1">
                    <DocumentTotals
                        title="金额摘要（含税）"
                        items={[
                            {
                                id: "erp",
                                label: "ERP 计算金额",
                                value: (
                                    <MoneyValue
                                        value={row.erpAmountGross}
                                        taxBasis="gross"
                                    />
                                ),
                                basis: "含税",
                            },
                            {
                                id: "bill",
                                label: "供应商账单金额",
                                value: row.supplierAmountGross ? (
                                    <MoneyValue
                                        value={row.supplierAmountGross}
                                        taxBasis="gross"
                                    />
                                ) : (
                                    "账单未同步"
                                ),
                                basis: "含税",
                            },
                            {
                                id: "diff",
                                label: "差异",
                                value: row.differenceAmountGross ? (
                                    <MoneyValue
                                        value={row.differenceAmountGross}
                                        taxBasis="gross"
                                    />
                                ) : (
                                    "—"
                                ),
                                warning: row.differenceDirectionLabel,
                            },
                        ]}
                    />
                    <div className="flex flex-wrap gap-2 text-sm text-muted-foreground">
                        <span>
                            经办 {row.preparedByLabel} · 复核{" "}
                            {row.reviewedByLabel}
                        </span>
                        <BusinessStatusBadge
                            context="list"
                            label={row.statusLabel}
                            tone={row.statusTone}
                        />
                    </div>
                    <div className="flex flex-wrap gap-2">
                        <Button
                            id="supplier-settlements-list-preview-open"
                            type="button"
                            onClick={() => onOpen(row.statementId)}
                        >
                            查看详情
                        </Button>
                        {row.unresolvedDifferenceCount > 0 ? (
                            <Button
                                id="supplier-settlements-list-preview-open-differences"
                                type="button"
                                variant="secondary"
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
                    </div>
                    <p className="text-xs text-muted-foreground">
                        键盘：列表 Enter
                        打开预览；详情页可继续提交复核并查询处理结果。
                    </p>
                </div>
            ) : (
                <div className="flex flex-col items-start gap-3 p-5">
                    <p className="text-sm text-muted-foreground">
                        未找到预览行，可能已被移出当前筛选范围。
                    </p>
                    <Button
                        id="supplier-settlements-list-preview-close"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => patchUrl({ preview: undefined })}
                    >
                        关闭预览
                    </Button>
                </div>
            )}
        </QuickPreviewSheet>
    )
}
