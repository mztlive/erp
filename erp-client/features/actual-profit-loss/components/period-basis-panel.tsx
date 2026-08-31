import { surfacePanelClassName } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import { OptionCombobox } from "@/components/business"
import type { ProfitLossPeriodBasisConfig } from "@/features/actual-profit-loss/types"
import type { ProfitLossUrlPatch } from "@/features/actual-profit-loss/hooks/use-actual-profit-loss-page"
import {
    parsePreset,
    resolvePeriod,
} from "@/features/actual-profit-loss/lib/url-state"

export function PeriodBasisPanel({
    presetRaw,
    from,
    to,
    periodBasis,
    basisConfig,
    periodBasisValid,
    patchUrl,
}: {
    presetRaw: string
    from: string
    to: string
    periodBasis: string
    basisConfig: ProfitLossPeriodBasisConfig
    periodBasisValid: boolean
    patchUrl: (
        patch: ProfitLossUrlPatch,
        options?: { replace?: boolean },
    ) => void
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>统计期间与归属口径</CardTitle>
                <CardDescription>
                    查询与导出仅按此处明确的期间与归属口径执行。
                </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-3 pt-4 sm:flex-row sm:flex-wrap sm:items-end">
                <div className="space-y-1.5">
                    <Label htmlFor="actual-profit-loss-period-preset">
                        期间快捷
                    </Label>
                    <OptionCombobox
                        id="actual-profit-loss-period-preset"
                        value={presetRaw}
                        onValueChange={(v) => {
                            if (!v) return
                            const preset = parsePreset(v)
                            const range = resolvePeriod(preset)
                            patchUrl({
                                periodPreset: preset,
                                from: range.from,
                                to: range.to,
                                page: null,
                            })
                        }}
                        options={[
                            { value: "", label: "自定义" },
                            { value: "month-to-date", label: "本月迄今" },
                            { value: "last-month", label: "上月" },
                            { value: "quarter-to-date", label: "本季迄今" },
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                        aria-label="期间快捷"
                        placeholder="期间快捷"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="actual-profit-loss-period-from">从</Label>
                    <DatePicker
                        id="actual-profit-loss-period-from"
                        className="w-[10.5rem]"
                        value={from || undefined}
                        onValueChange={(next) => {
                            patchUrl({
                                from: next || null,
                                periodPreset: null,
                                page: null,
                            })
                        }}
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="actual-profit-loss-period-to">至</Label>
                    <DatePicker
                        id="actual-profit-loss-period-to"
                        className="w-[10.5rem]"
                        value={to || undefined}
                        onValueChange={(next) => {
                            patchUrl({
                                to: next || null,
                                periodPreset: null,
                                page: null,
                            })
                        }}
                    />
                </div>
                <div className="space-y-1.5">
                    <Label>已解析期间</Label>
                    <p className="num text-sm font-medium">
                        {from} ~ {to}
                    </p>
                </div>
                <div className="min-w-[16rem] flex-1 space-y-1.5">
                    <Label htmlFor="actual-profit-loss-period-basis">
                        期间归属口径
                    </Label>
                    <OptionCombobox
                        id="actual-profit-loss-period-basis"
                        value={periodBasis || ""}
                        onValueChange={(v) => {
                            patchUrl({ periodBasis: v || null, page: null })
                        }}
                        options={[
                            {
                                value: "",
                                label: basisConfig.configuredPeriodBasis
                                    ? "请确认归属口径"
                                    : "请显式选择归属口径（未配置）",
                            },
                            ...basisConfig.allowedPeriodBases.map((opt) => ({
                                value: opt.code,
                                label:
                                    opt.label +
                                    (basisConfig.configuredPeriodBasis ===
                                    opt.code
                                        ? "（默认口径）"
                                        : ""),
                            })),
                        ]}
                        className="w-full min-w-[16rem]"
                        size="sm"
                        allowClear={false}
                        aria-label="期间归属口径"
                        placeholder="请选择归属口径"
                    />
                </div>
                {periodBasisValid ? (
                    <Badge variant="secondary">
                        口径已明确 · {basisConfig.configurationVersion}
                    </Badge>
                ) : (
                    <Badge variant="destructive">分析与导出已阻断</Badge>
                )}
            </CardContent>
        </Card>
    )
}
