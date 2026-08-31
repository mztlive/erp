import {
    BusinessDiffPanel,
    BusinessEmptyState,
    BusinessStatusBadge,
    MoneyValue,
    surfaceInsetClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
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
            <BusinessEmptyState
                kind="no-data"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title="无差异"
                description="当前结算单没有差异记录，明细金额核对一致时可直接进入复核。"
            />
        )
    }

    return (
        <div className="grid gap-4 xl:grid-cols-[16rem_minmax(0,1fr)]">
            <Card
                size="sm"
                className={cn(surfaceInsetClassName, "shadow-none ring-0")}
            >
                <CardHeader className="rounded-t-lg border-b border-grid py-3">
                    <CardTitle className="text-base">差异列表</CardTitle>
                </CardHeader>
                <CardContent className="space-y-1 p-2">
                    {differences.map((d) => (
                        <button
                            key={d.differenceId}
                            id={`supplier-settlements-differences-item-${toAutomationIdSegment(d.differenceId)}`}
                            type="button"
                            className={cn(
                                "flex w-full flex-col rounded-md px-2 py-2 text-left text-sm hover:bg-foreground/5",
                                activeDiff?.differenceId === d.differenceId
                                    ? "bg-card font-medium shadow-sm ring-1 ring-foreground/10"
                                    : "text-muted-foreground",
                            )}
                            onClick={() => onSelect(d.differenceId)}
                        >
                            <span className="font-medium">{d.typeLabel}</span>
                            <span className="text-xs text-muted-foreground">
                                {d.statusLabel}
                                {d.requiresProcurementEvidence
                                    ? " · 待举证"
                                    : ""}
                                {d.blocking ? " · 阻断" : ""}
                            </span>
                            {d.amountGross ? (
                                <span className="mt-0.5">
                                    <MoneyValue
                                        value={d.amountGross}
                                        className="num text-xs font-semibold text-warning-soft-foreground"
                                    />
                                </span>
                            ) : null}
                        </button>
                    ))}
                </CardContent>
            </Card>

            {activeDiff ? (
                <div className="space-y-4">
                    <Card
                        size="sm"
                        className={cn(
                            surfaceInsetClassName,
                            "shadow-none ring-0",
                        )}
                    >
                        <CardHeader className="rounded-t-lg border-b border-grid py-3">
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div>
                                    <CardTitle className="text-base">
                                        {activeDiff.typeLabel}
                                    </CardTitle>
                                    <CardDescription>
                                        {activeDiff.amountDirectionLabel}
                                        {activeDiff.amountGross ? (
                                            <span className="mt-1 block text-base font-semibold text-warning-soft-foreground">
                                                <MoneyValue
                                                    value={
                                                        activeDiff.amountGross
                                                    }
                                                    taxBasis="gross"
                                                />
                                            </span>
                                        ) : null}
                                    </CardDescription>
                                </div>
                                <BusinessStatusBadge
                                    context="list"
                                    label={activeDiff.statusLabel}
                                    tone={activeDiff.statusTone}
                                />
                                {activeDiff.requiresProcurementEvidence ? (
                                    <Badge variant="outline">需采购举证</Badge>
                                ) : null}
                                {activeDiff.blocking ? (
                                    <Badge variant="destructive">阻塞</Badge>
                                ) : null}
                            </div>
                        </CardHeader>
                        <CardContent className="space-y-3 pt-4">
                            <div className="grid gap-2 sm:grid-cols-2 text-sm">
                                <div
                                    className={cn(surfaceInsetClassName, "p-3")}
                                >
                                    <div className="text-xs text-muted-foreground">
                                        ERP 侧
                                    </div>
                                    <div>{activeDiff.erpSideLabel}</div>
                                    {activeDiff.erpSideAmount ? (
                                        <MoneyValue
                                            value={activeDiff.erpSideAmount}
                                            taxBasis="gross"
                                        />
                                    ) : null}
                                </div>
                                <div
                                    className={cn(surfaceInsetClassName, "p-3")}
                                >
                                    <div className="text-xs text-muted-foreground">
                                        供应商侧
                                    </div>
                                    <div>{activeDiff.supplierSideLabel}</div>
                                    {activeDiff.supplierSideAmount ? (
                                        <MoneyValue
                                            value={
                                                activeDiff.supplierSideAmount
                                            }
                                            taxBasis="gross"
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
                                <h4 className="mb-2 text-sm font-medium">
                                    已登记证据
                                </h4>
                                {activeDiff.evidence.length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
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
                                                    "px-3 py-2 text-sm",
                                                )}
                                            >
                                                <div className="font-medium">
                                                    {e.label} ·{" "}
                                                    {e.by.displayName}
                                                </div>
                                                <div className="text-muted-foreground">
                                                    {e.comment}
                                                </div>
                                                <div className="text-xs text-muted-foreground">
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
                                        {formatDateTime(
                                            activeDiff.resolution.at,
                                            "default",
                                        )}{" "}
                                        · 成本预览{" "}
                                        <MoneyValue
                                            value={
                                                activeDiff.resolution
                                                    .costImpactPreview ?? "0.00"
                                            }
                                            taxBasis="gross"
                                        />
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            <Separator />
                            <div className="flex flex-wrap gap-2">
                                {allowed.has("APPEND_EVIDENCE") ? (
                                    <Button
                                        id="supplier-settlements-differences-append-evidence"
                                        type="button"
                                        size="sm"
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
                        </CardContent>
                    </Card>
                </div>
            ) : null}
        </div>
    )
}
