"use client"

import { CheckIcon } from "lucide-react"
import { cn } from "@/lib/utils"

/** 步骤只表达进度；继续与返回由各步操作区负责。 */
export function ReceiptRegistrationSteps({ step }: { step: 1 | 2 }) {
    return (
        <ol
            aria-label="回款登记步骤"
            className="flex flex-wrap items-center gap-x-8 gap-y-3 border-b pb-5"
        >
            {["填写到账信息", "核销应收并提交"].map((label, index) => {
                const current = step === index + 1
                const completed = step > index + 1
                return (
                    <li
                        key={label}
                        aria-current={current ? "step" : undefined}
                        className={cn(
                            "flex items-center gap-2 text-sm",
                            current
                                ? "font-medium text-foreground"
                                : "text-muted-foreground",
                        )}
                    >
                        <span
                            className={cn(
                                "flex size-7 shrink-0 items-center justify-center rounded-full border text-xs",
                                current &&
                                    "border-primary bg-primary text-primary-foreground",
                            )}
                        >
                            {completed ? (
                                <CheckIcon
                                    className="size-4"
                                    aria-hidden="true"
                                />
                            ) : (
                                index + 1
                            )}
                        </span>
                        {label}
                        {completed ? (
                            <span className="sr-only">已完成</span>
                        ) : null}
                    </li>
                )
            })}
        </ol>
    )
}
