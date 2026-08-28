"use client"

import { BanIcon, ClipboardCheckIcon, SaveIcon } from "lucide-react"

import { DocumentHeader } from "@/components/business"
import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import type { ProductEditorFormValues } from "@/features/master-data/lib/product-editor-model"
import type { MasterDataCenterView } from "@/features/master-data/types"

type ProductDetailHeaderProps = {
    isCreate: boolean
    data: MasterDataCenterView | null | undefined
    title: string
    hasUpdatePermission: boolean
    canDisable: boolean
    disableBlocker: { message: string } | undefined
    setDisableOpen: (open: boolean) => void
    canRevise: boolean
    pending: boolean
    runLocalCheck: (values: ProductEditorFormValues) => void
    values: ProductEditorFormValues
}

function ProductDetailHeader({
    isCreate,
    data,
    title,
    hasUpdatePermission,
    canDisable,
    disableBlocker,
    setDisableOpen,
    canRevise,
    pending,
    runLocalCheck,
    values,
}: ProductDetailHeaderProps) {
    return (
        <DocumentHeader
            density="compact"
            title={title}
            documentNumber={isCreate ? "待生成" : data?.stableNo || "—"}
            primaryStatus={
                !isCreate && data
                    ? {
                          label: data.lifecycleStatusLabel,
                          tone: data.lifecycleTone,
                      }
                    : {
                          label: "待创建",
                          tone: "neutral",
                      }
            }
            version={
                !isCreate && data ? data.currentRevision.revisionNo : undefined
            }
            meta={
                !isCreate && data ? (
                    <span className="num">
                        {formatEffectiveRange(
                            data.currentRevision.effectiveFrom,
                            data.currentRevision.effectiveTo,
                        )}
                    </span>
                ) : undefined
            }
            statuses={
                !isCreate && data
                    ? [
                          {
                              id: "timing",
                              label: masterDataCopy.centerVersionState,
                              status: {
                                  label: data.revisionTimingLabel,
                                  tone:
                                      data.revisionTiming === "FUTURE"
                                          ? "warning"
                                          : "info",
                              },
                          },
                      ]
                    : undefined
            }
            secondaryActions={
                <>
                    {!isCreate && data ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!canDisable}
                            title={
                                !hasUpdatePermission
                                    ? "当前账号没有维护商品资料的权限。"
                                    : disableBlocker?.message
                            }
                            onClick={() => setDisableOpen(true)}
                        >
                            <BanIcon data-icon="inline-start" aria-hidden />
                            {masterDataCopy.actionDisable}
                        </Button>
                    ) : null}
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={!canRevise || pending}
                        onClick={() => runLocalCheck(values)}
                    >
                        <ClipboardCheckIcon
                            data-icon="inline-start"
                            aria-hidden
                        />
                        填写检查
                    </Button>
                </>
            }
            primaryAction={
                <Button
                    type="submit"
                    size="sm"
                    disabled={!canRevise || pending}
                >
                    <SaveIcon data-icon="inline-start" aria-hidden />
                    {pending
                        ? "提交中…"
                        : isCreate
                          ? masterDataCopy.createSubmit
                          : masterDataCopy.reviseSubmit}
                </Button>
            }
        />
    )
}

export { ProductDetailHeader }
