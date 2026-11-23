"use client"

import {
    ChevronRightIcon,
    ClipboardListIcon,
    FileTextIcon,
    SquarePenIcon,
} from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { FileUpload } from "@/components/ui/file-upload"
import { contractPdfError } from "@/features/contracts/lib/pdf"

/** 开单前确定业务依据；左侧主推合同归档带入，右侧直接选择已有合同、无合同或询价带入。 */
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
    const alternatives = [
        {
            basis: "contract" as const,
            icon: FileTextIcon,
            title: "选择已有合同",
            description: "从已归档的有效合同中选择，自动带入客户与结算条款",
        },
        {
            basis: "evidence" as const,
            icon: SquarePenIcon,
            title: "先开单，后补合同",
            description: "选择客户并上传开单凭证，签署合同后在详情页补录",
        },
        ...(onSelectInquiry
            ? [
                  {
                      basis: "inquiry" as const,
                      icon: ClipboardListIcon,
                      title: "从询价单带入",
                      description:
                          "沿用已接受的询价结果，自动带入客户、商品与配送范围",
                  },
              ]
            : []),
    ]

    return (
        <section
            className="min-w-0 py-6 lg:py-8"
            aria-labelledby="sales-orders-create-entry-title"
        >
            <div className="max-w-5xl space-y-6">
                <div className="space-y-1">
                    <h2
                        id="sales-orders-create-entry-title"
                        className="text-section-title font-semibold tracking-tight text-foreground"
                    >
                        选择开单方式
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        上传或关联已签署合同可自动带入客户、结算主体及付款开票条款；也可先凭业务凭证开单。
                    </p>
                </div>

                <div className="grid grid-cols-1 items-start gap-7 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)] lg:gap-9">
                    <div className="min-w-0 space-y-3">
                        <div className="flex items-center gap-2">
                            <h3 className="font-semibold text-foreground">
                                上传合同并开单
                            </h3>
                            <span className="rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-soft-foreground">
                                推荐
                            </span>
                        </div>
                        <FileUpload
                            idPrefix="sales-orders-create-entry-upload"
                            selectId="sales-orders-create-entry-upload"
                            density="hero"
                            controlClassName="min-h-56 gap-4 border-border/90 bg-surface-sunken/70 py-8 sm:min-h-60 lg:min-h-64"
                            label="拖拽已签署合同 PDF 到这里"
                            selectLabel="上传合同"
                            description="支持单个 PDF，不超过 20 MB"
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
                            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-info-border/70 bg-info-soft/50 px-3.5 py-2.5 text-body-compact">
                                <p className="min-w-0 break-all text-foreground">
                                    已选合同：{selectedFileName}
                                </p>
                                <Button
                                    id="sales-orders-create-entry-resume-import"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.inlineLinkButton
                                    }
                                    onClick={onResumeImport}
                                >
                                    查看导入进度
                                </Button>
                            </div>
                        ) : null}
                        <p className="text-xs text-muted-foreground">
                            合同识别结果需人工核对，确认归档后自动带入开单信息。
                        </p>
                    </div>

                    <div className="min-w-0 space-y-3 border-t border-border/60 pt-6 lg:border-t-0 lg:border-l lg:border-border/60 lg:pt-0 lg:pl-9">
                        <h3 className="font-semibold text-foreground">
                            其他开单方式
                        </h3>
                        <div className="space-y-2.5">
                            {alternatives.map(
                                ({ basis, icon: Icon, title, description }) => (
                                    <button
                                        key={basis}
                                        id={`sales-orders-create-entry-${basis}`}
                                        type="button"
                                        className="group flex w-full cursor-pointer items-center gap-3.5 rounded-lg border border-border/80 bg-card px-4 py-3.5 text-left transition-colors duration-150 hover:border-primary/40 hover:bg-sidebar-accent/35 focus-visible:border-primary focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
                                        disabled={
                                            basis === "inquiry" &&
                                            inquiryDisabled
                                        }
                                        onClick={() =>
                                            basis === "inquiry"
                                                ? onSelectInquiry?.()
                                                : onSelectBasis(basis)
                                        }
                                    >
                                        <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted/75 text-foreground/80 transition-colors group-hover:bg-info-soft group-hover:text-primary">
                                            <Icon
                                                className="size-4.5"
                                                strokeWidth={1.75}
                                                aria-hidden="true"
                                            />
                                        </span>
                                        <span className="min-w-0 flex-1 space-y-0.5">
                                            <span className="block font-medium text-foreground group-hover:text-primary">
                                                {title}
                                            </span>
                                            <span className="block text-xs text-muted-foreground">
                                                {description}
                                            </span>
                                        </span>
                                        <ChevronRightIcon
                                            className="size-4 shrink-0 text-muted-foreground/70 transition-transform duration-150 group-hover:translate-x-0.5 group-hover:text-primary"
                                            aria-hidden="true"
                                        />
                                    </button>
                                ),
                            )}
                        </div>
                    </div>
                </div>
            </div>
        </section>
    )
}
