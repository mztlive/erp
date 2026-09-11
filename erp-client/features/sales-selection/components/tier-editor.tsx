/**
 * 档位编辑器：套餐形态的档位规则增删改。
 * 纯 UI 状态组件，金额保持字符串，数量用整数输入。
 */

"use client"

import * as React from "react"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { TierRuleInput } from "@/features/sales-selection/types"

/** 空档位草稿。 */
const emptyTier = (): TierRuleInput => ({
    name: "",
    target_amount: "",
    tolerance: "0",
    expected_count: 5,
    sku_count: 3,
})

/**
 * 把整数输入解析为数字（失败回退默认值）。
 * @param raw 输入原文
 * @param fallback 回退值
 */
const parseCountInput = (raw: string, fallback: number): number => {
    const parsed = parseInt(raw.trim(), 10)
    return Number.isSafeInteger(parsed) ? parsed : fallback
}

/**
 * 档位编辑器。
 * @param value 当前档位
 * @param onChange 档位变化
 * @param disabled 是否只读
 */
export const TierEditor = ({
    value,
    onChange,
    disabled = false,
}: {
    value: readonly TierRuleInput[]
    onChange: (next: TierRuleInput[]) => void
    disabled?: boolean
}) => {
    const handlePatch = React.useCallback(
        (index: number, part: Partial<TierRuleInput>) => {
            onChange(
                value.map((tier, position) =>
                    position === index ? { ...tier, ...part } : tier,
                ),
            )
        },
        [onChange, value],
    )

    const handleAdd = React.useCallback(() => {
        if (value.length >= 10) return
        onChange([...value, emptyTier()])
    }, [onChange, value])

    const handleRemove = React.useCallback(
        (index: number) => {
            onChange(value.filter((_, position) => position !== index))
        },
        [onChange, value],
    )

    return (
        <div className="flex flex-col gap-3">
            {value.map((tier, index) => (
                <Card key={`tier-${index}`}>
                    <CardContent className="grid grid-cols-1 gap-3 p-4 sm:grid-cols-2">
                        <div className="grid gap-1.5 sm:col-span-2">
                            <div className="flex items-center justify-between">
                                <span className="text-sm font-medium">
                                    档位 {index + 1}
                                </span>
                                <Button
                                    id={`sales-selection-tier-${index}-remove`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    disabled={disabled || value.length <= 1}
                                    onClick={() => handleRemove(index)}
                                >
                                    删除
                                </Button>
                            </div>
                        </div>
                        <div className="grid gap-1.5">
                            <Label
                                htmlFor={`sales-selection-tier-${index}-name`}
                            >
                                名称
                            </Label>
                            <Input
                                id={`sales-selection-tier-${index}-name`}
                                value={tier.name}
                                disabled={disabled}
                                onChange={(event) =>
                                    handlePatch(index, {
                                        name: event.target.value,
                                    })
                                }
                                placeholder="如：100 元档"
                                autoComplete="off"
                            />
                        </div>
                        <div className="grid grid-cols-2 gap-3">
                            <div className="grid gap-1.5">
                                <Label
                                    htmlFor={`sales-selection-tier-${index}-target`}
                                >
                                    目标金额 · 元
                                </Label>
                                <Input
                                    id={`sales-selection-tier-${index}-target`}
                                    value={tier.target_amount}
                                    disabled={disabled}
                                    inputMode="decimal"
                                    onChange={(event) =>
                                        handlePatch(index, {
                                            target_amount: event.target.value,
                                        })
                                    }
                                    placeholder="100.00"
                                    autoComplete="off"
                                />
                            </div>
                            <div className="grid gap-1.5">
                                <Label
                                    htmlFor={`sales-selection-tier-${index}-tolerance`}
                                >
                                    容差 · 元
                                </Label>
                                <Input
                                    id={`sales-selection-tier-${index}-tolerance`}
                                    value={tier.tolerance}
                                    disabled={disabled}
                                    inputMode="decimal"
                                    onChange={(event) =>
                                        handlePatch(index, {
                                            tolerance: event.target.value,
                                        })
                                    }
                                    placeholder="5.00"
                                    autoComplete="off"
                                />
                            </div>
                        </div>
                        <div className="grid gap-1.5">
                            <Label
                                htmlFor={`sales-selection-tier-${index}-count`}
                            >
                                套餐数量（1–20）
                            </Label>
                            <Input
                                id={`sales-selection-tier-${index}-count`}
                                value={String(tier.expected_count)}
                                disabled={disabled}
                                inputMode="numeric"
                                onChange={(event) =>
                                    handlePatch(index, {
                                        expected_count: parseCountInput(
                                            event.target.value,
                                            tier.expected_count,
                                        ),
                                    })
                                }
                                autoComplete="off"
                            />
                        </div>
                        <div className="grid gap-1.5">
                            <Label
                                htmlFor={`sales-selection-tier-${index}-sku`}
                            >
                                每套餐件数（2–8）
                            </Label>
                            <Input
                                id={`sales-selection-tier-${index}-sku`}
                                value={String(tier.sku_count)}
                                disabled={disabled}
                                inputMode="numeric"
                                onChange={(event) =>
                                    handlePatch(index, {
                                        sku_count: parseCountInput(
                                            event.target.value,
                                            tier.sku_count,
                                        ),
                                    })
                                }
                                autoComplete="off"
                            />
                        </div>
                    </CardContent>
                </Card>
            ))}
            <Button
                id="sales-selection-tier-add"
                type="button"
                variant="outline"
                disabled={disabled || value.length >= 10}
                onClick={handleAdd}
            >
                添加档位
            </Button>
        </div>
    )
}
