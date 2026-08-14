import {
    OptionCombobox,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import {
    CustomerSearchCombobox,
    SalesOrderSearchCombobox,
} from "@/features/entity-selectors"
import type {
    CardBusinessDimension,
    CoverageFilter,
    DateBasis,
    ExpiryState,
    PeriodPreset,
} from "../types"
import { COVERAGE_FILTER_LABEL, DIMENSION_LABEL } from "../types"

export interface CardBusinessFilterBarProps {
    periodPresetValue: string
    from: string
    to: string
    dateBasis: DateBasis | ""
    dateBasisOptions: readonly { value: string; label: string }[]
    customerId: string | undefined
    salesOrderId: string | undefined
    canReadAllCustomers: boolean
    costBasisValue: string
    expiryState: ExpiryState
    coverage: CoverageFilter
    dimension: CardBusinessDimension
    hasActiveFilters: boolean
    onPresetChange: (preset: PeriodPreset) => void
    onFromChange: (next: string) => void
    onToChange: (next: string) => void
    onDateBasisChange: (next: string | null) => void
    onCustomerChange: (id?: string) => void
    onSalesOrderChange: (id?: string) => void
    onCostBasisChange: (next: string | null) => void
    onExpiryChange: (next: string | null) => void
    onCoverageChange: (next: string | null) => void
    onDimensionChange: (next: string | null) => void
    onClearFilters: () => void
}

export function CardBusinessFilterBar({
    periodPresetValue,
    from,
    to,
    dateBasis,
    dateBasisOptions,
    customerId,
    salesOrderId,
    canReadAllCustomers,
    costBasisValue,
    expiryState,
    coverage,
    dimension,
    hasActiveFilters,
    onPresetChange,
    onFromChange,
    onToChange,
    onDateBasisChange,
    onCustomerChange,
    onSalesOrderChange,
    onCostBasisChange,
    onExpiryChange,
    onCoverageChange,
    onDimensionChange,
    onClearFilters,
}: CardBusinessFilterBarProps) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardContent className="flex flex-col gap-3 pt-4 sm:flex-row sm:flex-wrap sm:items-end">
                <div className="space-y-1.5">
                    <Label htmlFor="w28-preset">期间快捷</Label>
                    <OptionCombobox
                        id="w28-preset"
                        value={periodPresetValue}
                        onValueChange={(v) => {
                            if (!v) return
                            onPresetChange(v as PeriodPreset)
                        }}
                        options={[
                            { value: "", label: "自定义" },
                            { value: "month-to-date", label: "本月至今" },
                            { value: "last-month", label: "上月" },
                            { value: "quarter-to-date", label: "本季至今" },
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                        aria-label="期间快捷"
                        placeholder="期间快捷"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-filter-from">从</Label>
                    <DatePicker
                        className="w-[10.5rem]"
                        value={from || undefined}
                        onValueChange={(next) => onFromChange(next ?? "")}
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-filter-to">至</Label>
                    <DatePicker
                        className="w-[10.5rem]"
                        value={to || undefined}
                        onValueChange={(next) => onToChange(next ?? "")}
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-filter-basis">日期口径</Label>
                    <OptionCombobox
                        id="w28-filter-basis"
                        value={dateBasis}
                        onValueChange={onDateBasisChange}
                        options={dateBasisOptions}
                        className="w-[12rem]"
                        size="sm"
                        allowClear={false}
                        aria-label="日期口径"
                        placeholder="日期口径"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-customer">客户</Label>
                    <CustomerSearchCombobox
                        value={customerId}
                        onValueChange={onCustomerChange}
                        purpose="filter"
                        scope={
                            canReadAllCustomers ? "all_authorized" : "assigned"
                        }
                        className="min-w-48"
                        placeholder="全部客户"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-so">销售单</Label>
                    <SalesOrderSearchCombobox
                        value={salesOrderId}
                        onValueChange={onSalesOrderChange}
                        purpose="filter"
                        className="min-w-48"
                        placeholder="全部销售单"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-costBasis">成本口径</Label>
                    <OptionCombobox
                        id="w28-costBasis"
                        value={costBasisValue}
                        onValueChange={onCostBasisChange}
                        options={[
                            { value: "", label: "全部" },
                            { value: "ACTUAL", label: "实际成本" },
                            { value: "STANDARD", label: "标准成本" },
                            { value: "NONE", label: "无可用成本" },
                            {
                                value: "ACTUAL,STANDARD",
                                label: "实际 + 标准成本",
                            },
                        ]}
                        className="w-[11rem]"
                        size="sm"
                        allowClear={false}
                        aria-label="成本口径"
                        placeholder="成本口径"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-expiry">履约期限</Label>
                    <OptionCombobox
                        id="w28-expiry"
                        value={expiryState}
                        onValueChange={onExpiryChange}
                        options={[
                            { value: "all", label: "全部" },
                            { value: "active", label: "未到期" },
                            { value: "expired", label: "已到期" },
                        ]}
                        className="w-[8rem]"
                        size="sm"
                        allowClear={false}
                        aria-label="履约期限"
                        placeholder="履约期限"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-coverage">覆盖口径</Label>
                    <OptionCombobox
                        id="w28-coverage"
                        value={coverage}
                        onValueChange={onCoverageChange}
                        options={[
                            { value: "all", label: "全部覆盖状态" },
                            {
                                value: "below_threshold",
                                label: COVERAGE_FILTER_LABEL.below_threshold,
                            },
                            {
                                value: "none",
                                label: COVERAGE_FILTER_LABEL.none,
                            },
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                        aria-label="覆盖口径"
                        placeholder="全部覆盖状态"
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="w28-dimension">分析视角</Label>
                    <OptionCombobox
                        id="w28-dimension"
                        value={dimension}
                        onValueChange={onDimensionChange}
                        options={(
                            Object.keys(
                                DIMENSION_LABEL,
                            ) as CardBusinessDimension[]
                        ).map((k) => ({
                            value: k,
                            label: DIMENSION_LABEL[k],
                        }))}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                        aria-label="分析视角"
                        placeholder="分析视角"
                    />
                </div>
                {hasActiveFilters && (
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={onClearFilters}
                    >
                        清除筛选
                    </Button>
                )}
            </CardContent>
        </Card>
    )
}
