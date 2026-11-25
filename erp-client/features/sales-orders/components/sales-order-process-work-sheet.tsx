"use client"

import type { RefObject } from "react"
import { QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ProcessStep } from "../lib/sales-order-current-work"

/** 销售跟进只展示剩余事项、当前环节与责任人，不读取或链接采购明细。 */
export function SalesOrderProcessWorkSheet({
    step,
    prefix,
    finalFocus,
    onClose,
}: {
    step?: ProcessStep
    prefix: string
    finalFocus: RefObject<HTMLElement | null>
    onClose: () => void
}) {
    return (
        <QuickPreviewSheet
            idPrefix={`${prefix}-work-sheet`}
            size="preview"
            open={Boolean(step)}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            finalFocus={finalFocus}
            title={`${step?.title ?? "当前事项"} · 待办进度`}
            footer={
                <Button
                    id={`${prefix}-work-close`}
                    variant="outline"
                    onClick={onClose}
                >
                    关闭
                </Button>
            }
        >
            <div className="space-y-5">
                {step?.groups.map((group) => {
                    const current = group.tasks[0]
                    return (
                        <section
                            key={group.id}
                            className="space-y-3 border-b pb-5 last:border-0 last:pb-0"
                        >
                            <h3 className="font-semibold">
                                {group.label} · {group.count} 项
                            </h3>
                            <dl className="space-y-3 text-body-compact">
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        当前责任人
                                    </dt>
                                    <dd className="mt-1">{group.owner}</dd>
                                </div>
                                {current.current_node ? (
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            当前环节
                                        </dt>
                                        <dd className="mt-1">
                                            {current.current_node}
                                        </dd>
                                    </div>
                                ) : null}
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        待完成
                                    </dt>
                                    <dd className="mt-1">
                                        {current.owner_user_id
                                            ? current.instruction
                                            : "尚未分配处理人，需先明确责任人。"}
                                    </dd>
                                </div>
                            </dl>
                        </section>
                    )
                })}
                {!step?.groups.length ? (
                    <p className="text-xs text-muted-foreground">
                        当前环节暂无待处理事项。
                    </p>
                ) : null}
            </div>
        </QuickPreviewSheet>
    )
}
