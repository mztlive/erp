"use client"

import { CalendarRangeIcon } from "lucide-react"

import { PageHeader, surfacePanelClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import type { CustomerQualityPeriodPolicy } from "../types"

export function PeriodBlockerCard({
    periodPolicy,
    explicitFrom,
    explicitTo,
    onFromChange,
    onToChange,
    onApplyExplicit,
    onApplyPreset,
}: {
    periodPolicy: CustomerQualityPeriodPolicy | undefined
    explicitFrom: string
    explicitTo: string
    onFromChange: (value: string) => void
    onToChange: (value: string) => void
    onApplyExplicit: () => void
    onApplyPreset: (presetId: string, from: string, to: string) => void
}) {
    return (
        <>
            <PageHeader
                title="客户经营质量"
                description="未配置默认统计期间，请选择起止日期后开始分析。"
                breadcrumbs={[
                    {
                        id: "an",
                        label: "分析",
                        href: "/analytics/customer-quality",
                    },
                    { id: "cq", label: "客户经营质量", current: true },
                ]}
            />
            <Alert variant="warning">
                <CalendarRangeIcon aria-hidden="true" />
                <AlertTitle>请选择统计期间</AlertTitle>
                <AlertDescription>
                    尚未设置默认统计期间。选定期间后才会显示指标、图表与明细。
                </AlertDescription>
            </Alert>
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>显式期间</CardTitle>
                    <CardDescription>
                        选定后作为本页所有统计的唯一期间。
                    </CardDescription>
                </CardHeader>
                <CardContent className="flex flex-col gap-4 pt-4 sm:flex-row sm:items-end">
                    <div className="grid flex-1 gap-2 sm:grid-cols-2">
                        <div className="space-y-1.5">
                            <Label htmlFor="cq-from">开始日期</Label>
                            <DatePicker
                                value={explicitFrom || undefined}
                                onValueChange={(next) =>
                                    onFromChange(next ?? "")
                                }
                            />
                        </div>
                        <div className="space-y-1.5">
                            <Label htmlFor="cq-to">结束日期</Label>
                            <DatePicker
                                value={explicitTo || undefined}
                                onValueChange={(next) =>
                                    onToChange(next ?? "")
                                }
                            />
                        </div>
                    </div>
                    <div className="flex flex-col gap-2">
                        <Button
                            type="button"
                            disabled={
                                !explicitFrom ||
                                !explicitTo ||
                                (explicitFrom > explicitTo &&
                                    explicitFrom !== explicitTo)
                            }
                            onClick={onApplyExplicit}
                        >
                            开始分析
                        </Button>
                        {explicitFrom &&
                        explicitTo &&
                        explicitFrom > explicitTo ? (
                            <p className="text-xs text-destructive">
                                开始日期不能晚于结束日期，请调整后提交。
                            </p>
                        ) : null}
                    </div>
                </CardContent>
                {periodPolicy?.presets && periodPolicy.presets.length > 0 ? (
                    <CardContent className="border-t pt-4">
                        <p className="mb-2 text-sm text-muted-foreground">
                            或选择快捷期间：
                        </p>
                        <div className="flex flex-wrap gap-2">
                            {periodPolicy.presets.map((p) => (
                                <Button
                                    key={p.id}
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={() =>
                                        onApplyPreset(p.id, p.from, p.to)
                                    }
                                >
                                    {p.label}
                                </Button>
                            ))}
                        </div>
                    </CardContent>
                ) : null}
            </Card>
        </>
    )
}
