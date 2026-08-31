import Link from "next/link"

import { surfacePanelClassName } from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatMoneyDisplay } from "../../lib/presentation"
import type { CardBusinessAnalyticsView } from "../../types"

export interface BreakdownPanelProps {
    breakdowns: CardBusinessAnalyticsView["breakdowns"]
}

/** 类目 / 客户构成（排名不越过数据范围，全量口径）。 */
export function BreakdownPanel({ breakdowns }: BreakdownPanelProps) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>类目 / 客户构成</CardTitle>
                <CardDescription>
                    排名不越过数据范围。全量口径，不随明细筛选变化。
                </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 pt-4 sm:grid-cols-2">
                <div>
                    <h3 className="mb-2 text-sm font-medium">按类目</h3>
                    <ul className="space-y-2 text-sm">
                        {breakdowns.byCategory.map((item) => (
                            <li
                                key={item.id}
                                className="flex items-center justify-between gap-2"
                            >
                                <span>{item.label}</span>
                                <span className="num text-muted-foreground">
                                    {formatMoneyDisplay(item.consumptionGross)}{" "}
                                    ·{item.share}
                                </span>
                            </li>
                        ))}
                    </ul>
                </div>
                <div>
                    <h3 className="mb-2 text-sm font-medium">按客户</h3>
                    <ul className="space-y-2 text-sm">
                        {breakdowns.byCustomer.map((item) => (
                            <li
                                key={item.id}
                                className="flex items-center justify-between gap-2"
                            >
                                <Link
                                    id={`card-contracts-analytics-breakdown-customer-${toAutomationIdSegment(item.id)}`}
                                    href={`/sales/customers/${item.id}`}
                                    className="underline-offset-2 hover:underline"
                                >
                                    {item.label}
                                </Link>
                                <span className="num text-muted-foreground">
                                    {formatMoneyDisplay(item.consumptionGross)}{" "}
                                    ·{item.share}
                                </span>
                            </li>
                        ))}
                    </ul>
                </div>
            </CardContent>
        </Card>
    )
}
