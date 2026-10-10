"use client"

import * as React from "react"
import {
    ChevronRightIcon,
    FileIcon,
    FileTextIcon,
    InfoIcon,
    SquarePenIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { contractPdfError } from "@/features/contracts/lib/pdf"
import { cn } from "@/lib/utils"

/** 开单前只确定依据；合同核对归档后再展示销售表单。 */
export function SalesOrderCreateEntry({
    onUpload,
    onSelectBasis,
    selectedFileName,
    onResumeImport,
}: {
    onUpload: (file: File) => void
    onSelectBasis: (basis: "contract" | "evidence") => void
    selectedFileName?: string
    onResumeImport: () => void
}) {
    const inputRef = React.useRef<HTMLInputElement>(null)
    const [dragging, setDragging] = React.useState(false)
    const [error, setError] = React.useState<string | null>(null)
    const selectFiles = (files: File[]) => {
        if (!files.length) return
        const error =
            files.length !== 1
                ? "请每次上传一份合同 PDF"
                : contractPdfError(files[0])
        setError(error)
        if (!error) onUpload(files[0])
    }

    return (
        <section
            className="border-t border-grid"
            aria-labelledby="sales-orders-create-entry-title"
        >
            <div className="mx-auto w-full max-w-7xl px-1 py-8 sm:px-6 sm:py-10 lg:px-14 lg:py-12">
                <h2
                    id="sales-orders-create-entry-title"
                    className="text-xl font-semibold tracking-tight sm:text-2xl"
                >
                    导入合同，开始开单
                </h2>
                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                    上传已签署合同，核对归档后自动带入开单信息。
                </p>
                <div
                    id="sales-orders-create-entry-upload-dropzone"
                    className={cn(
                        "mt-7 flex min-h-64 flex-col items-center justify-center rounded-lg border border-dashed border-border bg-surface-sunken/50 px-5 py-10 text-center transition-colors sm:min-h-72 lg:mt-9 lg:min-h-90",
                        dragging && "border-ring bg-accent",
                    )}
                    onDragEnter={(event) => {
                        event.preventDefault()
                        setDragging(true)
                    }}
                    onDragOver={(event) => event.preventDefault()}
                    onDragLeave={(event) => {
                        if (
                            !event.currentTarget.contains(
                                event.relatedTarget as Node | null,
                            )
                        )
                            setDragging(false)
                    }}
                    onDrop={(event) => {
                        event.preventDefault()
                        setDragging(false)
                        selectFiles(Array.from(event.dataTransfer.files))
                    }}
                >
                    <FileIcon
                        className="mb-6 size-12 text-muted-foreground"
                        strokeWidth={1.5}
                        aria-hidden="true"
                    />
                    <p className="text-base font-semibold sm:text-lg">
                        拖拽合同 PDF 到这里
                    </p>
                    <p className="mt-2 text-sm text-muted-foreground">
                        或点击下方按钮选择文件
                    </p>
                    <input
                        id="sales-orders-create-entry-upload-input"
                        ref={inputRef}
                        type="file"
                        accept="application/pdf,.pdf"
                        className="sr-only"
                        aria-label="选择已签署合同 PDF"
                        aria-describedby="sales-orders-create-entry-upload-help sales-orders-create-entry-upload-error"
                        aria-invalid={Boolean(error)}
                        onChange={(event) => {
                            selectFiles(Array.from(event.target.files ?? []))
                            event.target.value = ""
                        }}
                    />
                    <Button
                        id="sales-orders-create-entry-upload"
                        type="button"
                        size="lg"
                        className="mt-6 min-w-44"
                        onClick={() => inputRef.current?.click()}
                    >
                        上传合同
                    </Button>
                    <p
                        id="sales-orders-create-entry-upload-help"
                        className="mt-3 text-xs text-muted-foreground"
                    >
                        仅支持单个 PDF，文件不超过 20 MB
                    </p>
                </div>
                <p
                    id="sales-orders-create-entry-upload-error"
                    role="alert"
                    className={cn("text-sm text-destructive", error && "mt-3")}
                >
                    {error}
                </p>
                {selectedFileName ? (
                    <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-sm">
                        <p className="min-w-0 break-all text-muted-foreground">
                            已选择：{selectedFileName}
                        </p>
                        <Button
                            id="sales-orders-create-entry-resume-import"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onResumeImport}
                        >
                            查看导入进度
                        </Button>
                    </div>
                ) : null}
                <div className="mt-5 flex items-start gap-2.5 rounded-lg bg-muted/50 px-4 py-3 text-sm leading-relaxed text-muted-foreground">
                    <InfoIcon
                        className="mt-0.5 size-4 shrink-0"
                        aria-hidden="true"
                    />
                    <p>
                        核对归档后，自动带入客户、结算主体、有效合同及结算条款。
                    </p>
                </div>
                <h3 className="mb-3 mt-9 text-base font-semibold sm:mt-10">
                    其他开单方式
                </h3>
                <div className="divide-y divide-border border-y border-border">
                    {[
                        {
                            basis: "contract" as const,
                            icon: FileTextIcon,
                            title: "选择已有合同",
                            description:
                                "从已归档的有效合同中选择，直接填写销售单",
                        },
                        {
                            basis: "evidence" as const,
                            icon: SquarePenIcon,
                            title: "先开单，后补合同",
                            description:
                                "选择客户与结算主体，上传业务凭证后开单",
                        },
                    ].map(({ basis, icon: Icon, title, description }) => (
                        <button
                            key={basis}
                            id={`sales-orders-create-entry-${basis}`}
                            type="button"
                            className="flex w-full items-center gap-4 px-3 py-5 text-left transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset sm:gap-5"
                            onClick={() => onSelectBasis(basis)}
                        >
                            <Icon
                                className="size-7 shrink-0"
                                strokeWidth={1.5}
                                aria-hidden="true"
                            />
                            <span className="min-w-0 flex-1 space-y-1.5">
                                <span className="block text-sm font-semibold">
                                    {title}
                                </span>
                                <span className="block text-sm leading-relaxed text-muted-foreground">
                                    {description}
                                </span>
                            </span>
                            <ChevronRightIcon
                                className="size-4 shrink-0 text-muted-foreground"
                                aria-hidden="true"
                            />
                        </button>
                    ))}
                </div>
                <p className="mt-6 text-xs leading-relaxed text-muted-foreground">
                    合同识别结果需人工核对，确认后才会归档。
                </p>
            </div>
        </section>
    )
}
