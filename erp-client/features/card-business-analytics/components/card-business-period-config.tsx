import { OptionCombobox, surfacePanelClassName } from "@/components/business"
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
import type { DateBasis } from "../types"

export interface CardBusinessPeriodConfigProps {
    explicitFrom: string
    explicitTo: string
    explicitDateBasis: DateBasis
    dateBasisOptions: readonly { value: string; label: string }[]
    onFromChange: (next: string) => void
    onToChange: (next: string) => void
    onDateBasisChange: (next: DateBasis) => void
    onApply: () => void
}

/** Q2：默认日期口径未配置时，要求用户显式选择完整期间与口径的卡片。 */
export function CardBusinessPeriodConfig({
    explicitFrom,
    explicitTo,
    explicitDateBasis,
    dateBasisOptions,
    onFromChange,
    onToChange,
    onDateBasisChange,
    onApply,
}: CardBusinessPeriodConfigProps) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>显式期间与日期口径</CardTitle>
                <CardDescription>
                    选择完整的期间与日期口径后才会发起查询；该选择将作用于全部指标与图表。
                </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4 pt-4">
                <div className="grid gap-3 sm:grid-cols-3">
                    <div className="space-y-1.5">
                        <Label htmlFor="card-contracts-analytics-period-config-from">
                            开始日期
                        </Label>
                        <DatePicker
                            id="card-contracts-analytics-period-config-from"
                            value={explicitFrom || undefined}
                            onValueChange={(next) => onFromChange(next ?? "")}
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="card-contracts-analytics-period-config-to">
                            结束日期
                        </Label>
                        <DatePicker
                            id="card-contracts-analytics-period-config-to"
                            value={explicitTo || undefined}
                            onValueChange={(next) => onToChange(next ?? "")}
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="card-contracts-analytics-period-config-date-basis">
                            日期口径
                        </Label>
                        <OptionCombobox
                            id="card-contracts-analytics-period-config-date-basis"
                            value={explicitDateBasis}
                            onValueChange={(v) =>
                                onDateBasisChange(
                                    (v ?? explicitDateBasis) as DateBasis,
                                )
                            }
                            options={dateBasisOptions}
                            className="w-full"
                            size="sm"
                            allowClear={false}
                            aria-label="日期口径"
                            placeholder="日期口径"
                        />
                    </div>
                </div>
                <div className="flex flex-col gap-2">
                    <Button
                        id="card-contracts-analytics-period-config-apply"
                        type="button"
                        disabled={
                            !explicitFrom ||
                            !explicitTo ||
                            !explicitDateBasis ||
                            (explicitFrom > explicitTo &&
                                explicitFrom !== explicitTo)
                        }
                        onClick={onApply}
                    >
                        开始分析
                    </Button>
                    {explicitFrom && explicitTo && explicitFrom > explicitTo ? (
                        <p className="text-xs text-destructive">
                            开始日期不能晚于结束日期，请调整后提交。
                        </p>
                    ) : null}
                </div>
            </CardContent>
        </Card>
    )
}
