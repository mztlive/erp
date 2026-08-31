"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

export function ChartFilterSummaryAlert({
    dimensionTitle,
    itemLabel,
    resultCount,
    onClear,
}: {
    dimensionTitle: string
    itemLabel: string
    resultCount: number
    onClear: () => void
}) {
    return (
        <Alert variant="info">
            <AlertTitle>图表筛选已生效</AlertTitle>
            <AlertDescription>
                <span aria-live="polite">
                    {dimensionTitle} · {itemLabel} · 结果{" "}
                    <span className="num font-medium">{resultCount}</span> 户
                </span>
                <Button
                    id="customers-quality-chart-clear"
                    type="button"
                    size="sm"
                    variant="ghost"
                    className="ml-2"
                    onClick={onClear}
                >
                    清除图表筛选
                </Button>
            </AlertDescription>
        </Alert>
    )
}
