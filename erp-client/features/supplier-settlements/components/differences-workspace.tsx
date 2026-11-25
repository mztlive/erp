import {
    BusinessDiffPanel,
    BusinessEmptyState,
    BusinessStatusBadge,
    MoneyValue,
    surfaceInsetClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { DetailWorkspaceSection } from "@/components/business/detail-workspace"
import { Separator } from "@/components/ui/separator"
import type { SettlementDifferenceView } from "@/features/supplier-settlements/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

export function DifferencesWorkspace({
    differences,
    activeDiff,
    onSelect,
    allowed,
    onResolve,
    onEvidence,
}: {
    differences: SettlementDifferenceView[]
    activeDiff: SettlementDifferenceView | null
    onSelect: (id: string) => void
    allowed: Set<string>
    onResolve: () => void
    onEvidence: () => void
}) {
    if (differences.length === 0) {
        return (
            <DetailWorkspaceSection width="form">
                <BusinessEmptyState
                    kind="no-data"
                    title="无差异"
                    description="当前结算单没有差异记录，明细金额核对一致时可直接进入复核。"
                />
            </DetailWorkspaceSection>
        )
    }

    return (
        <div className="@container/settlement-differences min-w-0">
            <div className="grid gap-4 @[840px]/settlement-differences:grid-cols-[16rem_minmax(0,1fr)]">
                <DetailWorkspaceSection title="差异列表">
                    <div className="flex flex-col gap-1">
                        {differences.map((d) => (
                            <button
                                key={d.differenceId}
                                id={`supplier-settlements-differences-item-${toAutomationIdSegment(d.differenceId)}`}
                                type="button"
                                className={cn(
                                    "flex w-full flex-col rounded-md px-2 py-2 text-left text-body-compact hover:bg-foreground/5",
                                    activeDiff?.differenceId === d.differenceId
                                        ? "bg-muted font-medium text-foreground"
                                        : "text-muted-foreground",
                                )}
                                onClick={() => onSelect(d.differenceId)}
                            >
                                <span className="font-medium">
                                    {d.typeLabel}
                                </span>
                                <span className="text-xs text-muted-foreground">
                                    {d.statusLabel}
                                    {d.requiresProcurementEvidence
                                        ? " · 待举证"
                                        : ""}
                                    {d.blocking ? " · 阻断" : ""}
                                </span>
                                {d.amountGross ? (
                                    <span className="mt-0.5 text-xs text-warning-soft-foreground">
                                        <MoneyValue
                                            value={d.amountGross}
                                            display="exact"
                                            inheritColor
                                        />
                                    </span>
                                ) : null}
                            </button>
                        ))}
                    </div>
                </DetailWorkspaceSection>

                {activeDiff ? (
                    <div className="space-y-4">
                        <DetailWorkspaceSection
                            className="border-t border-border/60 @[840px]/settlement-differences:border-t-0 @[840px]/settlement-differences:border-l @[840px]/settlement-differences:pl-5"
                            title={activeDiff.typeLabel}
                            actions={
                                <div className="flex flex-wrap items-center gap-1.5">
                                    <BusinessStatusBadge
                                        context="list"
                                        label={activeDiff.statusLabel}
                                        tone={activeDiff.statusTone}
                                    />
                                    {activeDiff.requiresProcurementEvidence ? (
                                        <BusinessStatusBadge
                                            context="list"
                                            tone="warning"
                                            label="需采购举证"
                                        />
                                    ) : null}
                                    {activeDiff.blocking ? (
                                        <BusinessStatusBadge
                                            context="list"
                                            tone="destructive"
                                            label="阻塞"
                                        />
                                    ) : null}
                                </div>
                            }
                            subtitle={activeDiff.amountDirectionLabel}
                        >
                            {activeDiff.amountGross ? (
                                <span className="text-warning-soft-foreground">
                                    <MoneyValue
                                        value={activeDiff.amountGross}
                                        display="exact"
                                        inheritColor
                                    />
                                </span>
                            ) : null}

                            <div className="grid gap-2 text-body-compact sm:grid-cols-2">
                                <div
                                    className={cn(surfaceInsetClassName, "p-3")}
                                >
                                    <div className="text-xs text-muted-foreground">
                                        ERP 侧（含税）
                                    </div>
                                    <div>{activeDiff.erpSideLabel}</div>
                                    {activeDiff.erpSideAmount ? (
                                        <MoneyValue
                                            value={activeDiff.erpSideAmount}
                                            display="exact"
                                        />
                                    ) : null}
                                </div>
                                <div
                                    className={cn(surfaceInsetClassName, "p-3")}
                                >
                                    <div className="text-xs text-muted-foreground">
                                        供应商侧（含税）
                                    </div>
                                    <div>{activeDiff.supplierSideLabel}</div>
                                    {activeDiff.supplierSideAmount ? (
                                        <MoneyValue
                                            value={
                                                activeDiff.supplierSideAmount
                                            }
                                            display="exact"
                                        />
                                    ) : null}
                                </div>
                            </div>

                            <BusinessDiffPanel
                                title="左右证据对比"
                                caption="字段级证据；原值只读不可为消差改写"
                                changes={activeDiff.leftFields.map((c) => ({
                                    id: c.id,
                                    field: c.field,
                                    before: c.before,
                                    after: c.after,
                                    note: c.note,
                                }))}
                            />

                            <div>
                                <h4 className="mb-2 text-body-compact font-medium">
                                    已登记证据
                                </h4>
                                {activeDiff.evidence.length === 0 ? (
                                    <p className="text-xs text-muted-foreground">
                                        尚无采购/供应商证据
                                        {activeDiff.requiresProcurementEvidence
                                            ? "（本差异需要采购协同）"
                                            : ""}
                                    </p>
                                ) : (
                                    <ul className="space-y-2">
                                        {activeDiff.evidence.map((e) => (
                                            <li
                                                key={e.evidenceId}
                                                className={cn(
                                                    surfaceInsetClassName,
                                                    "px-3 py-2 text-body-compact",
                                                )}
                                            >
                                                <div className="font-medium">
                                                    {e.label} ·{" "}
                                                    {e.by.displayName}
                                                </div>
                                                <div className="text-muted-foreground">
                                                    {e.comment}
                                                </div>
                                                <div className="num text-xs text-muted-foreground">
                                                    {formatDateTime(
                                                        e.at,
                                                        "default",
                                                    )}
                                                </div>
                                            </li>
                                        ))}
                                    </ul>
                                )}
                            </div>

                            {activeDiff.resolution ? (
                                <Alert variant="success">
                                    <AlertTitle>
                                        结论：
                                        {activeDiff.resolution.resolutionLabel}
                                    </AlertTitle>
                                    <AlertDescription>
                                        {activeDiff.resolution.by.displayName} ·{" "}
                                        <span className="num">
                                            {formatDateTime(
                                                activeDiff.resolution.at,
                                                "default",
                                            )}
                                        </span>{" "}
                                        · 成本预览{" "}
                                        <MoneyValue
                                            value={
                                                activeDiff.resolution
                                                    .costImpactPreview ?? "0.00"
                                            }
                                            display="exact"
                                        />
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            <Separator />
                            <div className="flex flex-wrap items-center gap-2">
                                {allowed.has("APPEND_EVIDENCE") ? (
                                    <Button
                                        id="supplier-settlements-differences-append-evidence"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={onEvidence}
                                    >
                                        追加采购证据
                                    </Button>
                                ) : null}
                                {allowed.has("RESOLVE_DIFFERENCE") &&
                                activeDiff.status === "PENDING" ? (
                                    <Button
                                        id="supplier-settlements-differences-resolve"
                                        type="button"
                                        size="sm"
                                        onClick={onResolve}
                                    >
                                        登记结论
                                    </Button>
                                ) : null}
                                {!allowed.has("RESOLVE_DIFFERENCE") ? (
                                    <span className="text-xs text-muted-foreground">
                                        当前不可登记差异结论
                                    </span>
                                ) : null}
                            </div>
                        </DetailWorkspaceSection>
                    </div>
                ) : null}
            </div>
        </div>
    )
}
