"use client"

import { CheckCircle2Icon, CircleIcon, InfoIcon } from "lucide-react"

import { Progress } from "@/components/ui/progress"
import { Spinner } from "@/components/ui/spinner"
import type { ContractImportStage } from "@/features/contracts/api/upload"
import { cn } from "@/lib/utils"

const STAGES: {
    key: ContractImportStage
    label: string
    description: string
}[] = [
    {
        key: "reading_file",
        label: "读取文件",
        description: "正在读取并检查合同原文件。",
    },
    {
        key: "ocr",
        label: "OCR 文字识别",
        description: "正在识别合同各页文字，完成后将提取合同字段。",
    },
    {
        key: "ai_extract",
        label: "AI 字段提取",
        description: "文字识别已完成，正在提取合同信息及原文依据。",
    },
    {
        key: "preparing_review",
        label: "整理核对结果",
        description: "字段提取已完成，正在整理并保存待核对结果。",
    },
]

export function importStageLabel(stage?: ContractImportStage | null) {
    return STAGES.find((item) => item.key === stage)?.label
}

export function ContractImportProgress({
    stage,
}: {
    stage?: ContractImportStage | null
}) {
    const current = STAGES.findIndex((item) => item.key === stage)
    const active = STAGES[current]
    return (
        <div
            className="rounded-lg bg-muted/50 p-5 sm:p-6"
            role="status"
            aria-live="polite"
        >
            <div className="flex items-center gap-3">
                <Spinner
                    className="size-5 motion-reduce:animate-none"
                    aria-hidden="true"
                />
                <h3 className="font-medium">
                    {active ? `正在${active.label}` : "正在识别合同"}
                </h3>
            </div>
            <div className="mt-4 space-y-4 sm:ml-8">
                <p className="text-sm leading-relaxed text-muted-foreground">
                    {active?.description ?? "任务正在处理，等待后端进度更新。"}
                </p>
                {active ? (
                    <>
                        <div className="flex items-center justify-between text-xs text-muted-foreground">
                            <span>识别进度</span>
                            <span>
                                已完成 {current} / {STAGES.length} 个步骤
                            </span>
                        </div>
                        <Progress
                            value={current}
                            max={STAGES.length}
                            aria-label="合同识别进度"
                            aria-valuetext={`已完成 ${current} 个步骤，正在${active.label}`}
                            className="[&_[data-slot=progress-track]]:h-1.5 [&_[data-slot=progress-track]]:bg-foreground/10 [&_[data-slot=progress-indicator]]:bg-foreground/50"
                        />
                        <ol className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                            {STAGES.map((item, index) => (
                                <li
                                    key={item.key}
                                    aria-current={
                                        index === current ? "step" : undefined
                                    }
                                    className={cn(
                                        "flex items-center gap-2 text-xs",
                                        index > current &&
                                            "text-muted-foreground",
                                    )}
                                >
                                    {index < current ? (
                                        <CheckCircle2Icon
                                            className="size-4 shrink-0 text-success"
                                            aria-hidden="true"
                                        />
                                    ) : index === current ? (
                                        <Spinner
                                            className="size-4 shrink-0 motion-reduce:animate-none"
                                            aria-hidden="true"
                                        />
                                    ) : (
                                        <CircleIcon
                                            className="size-4 shrink-0"
                                            aria-hidden="true"
                                        />
                                    )}
                                    <span className="sr-only">
                                        {index < current
                                            ? "已完成："
                                            : index === current
                                              ? "进行中："
                                              : "等待中："}
                                    </span>
                                    {item.label}
                                </li>
                            ))}
                        </ol>
                    </>
                ) : null}
                <p className="flex items-start gap-2 text-xs leading-relaxed text-muted-foreground">
                    <InfoIcon
                        className="mt-0.5 size-3.5 shrink-0"
                        aria-hidden="true"
                    />
                    可以关闭窗口，任务将在后台继续处理。
                </p>
            </div>
        </div>
    )
}
