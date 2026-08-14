"use client"

import type { CustomerQualityView } from "../types"

export function NatureDimensionList({
    dimension,
}: {
    dimension: CustomerQualityView["dimensions"][number]
}) {
    return (
        <div className="border-t pt-3">
            <p className="mb-2 text-sm font-medium">{dimension.title}</p>
            <ul className="grid gap-1 text-sm sm:grid-cols-2">
                {dimension.items.map((item) => (
                    <li
                        key={item.code}
                        className="flex justify-between gap-2 text-muted-foreground"
                    >
                        <span>
                            {item.label}
                            {item.code === "VOUCHER" ? (
                                <span className="ml-1 text-xs">
                                    （计规模/回款，不计盈亏）
                                </span>
                            ) : null}
                        </span>
                        <span className="num">
                            {item.value} · {item.share}
                        </span>
                    </li>
                ))}
            </ul>
        </div>
    )
}
