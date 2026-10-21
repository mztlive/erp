"use client"

import {
    ChevronRightIcon,
    ClipboardListIcon,
    FileTextIcon,
    InfoIcon,
    SquarePenIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { contractPdfError } from "@/features/contracts/lib/pdf"
import { FileUpload } from "@/components/ui/file-upload"

/** 开单前只确定依据；合同核对归档后再展示销售表单。 */
export function SalesOrderCreateEntry({
    onUpload,
    onSelectBasis,
    selectedFileName,
    onResumeImport,
    onSelectInquiry,
    inquiryDisabled = false,
}: {
    onUpload: (file: File) => void
    onSelectBasis: (basis: "contract" | "evidence") => void
    selectedFileName?: string
    onResumeImport: () => void
    onSelectInquiry?: () => void
    inquiryDisabled?: boolean
}) {
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
                <FileUpload
                    idPrefix="sales-orders-create-entry-upload"
                    selectId="sales-orders-create-entry-upload"
                    density="hero"
                    className="mt-7 lg:mt-9"
                    label="拖拽合同 PDF 到这里"
                    selectLabel="上传合同"
                    description="仅支持单个 PDF，文件不超过 20 MB"
                    accept="application/pdf,.pdf"
                    multiple={false}
                    maxFileSize={20 * 1024 * 1024}
                    validateFiles={(files) =>
                        contractPdfError(files[0] ?? null)
                    }
                    onFilesSelected={(files) => {
                        if (files[0]) onUpload(files[0])
                    }}
                />
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
                        ...(onSelectInquiry
                            ? [
                                  {
                                      basis: "inquiry" as const,
                                      icon: ClipboardListIcon,
                                      title: "从询价单带入",
                                      description:
                                          "选择已接受的询价结果，带入客户、商品与配送信息",
                                  },
                              ]
                            : []),
                    ].map(({ basis, icon: Icon, title, description }) => (
                        <button
                            key={basis}
                            id={`sales-orders-create-entry-${basis}`}
                            type="button"
                            className="flex w-full items-center gap-4 px-3 py-5 text-left transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset disabled:pointer-events-none disabled:opacity-50 sm:gap-5"
                            disabled={basis === "inquiry" && inquiryDisabled}
                            onClick={() =>
                                basis === "inquiry"
                                    ? onSelectInquiry?.()
                                    : onSelectBasis(basis)
                            }
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
