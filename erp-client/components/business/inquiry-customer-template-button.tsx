"use client"

import type * as React from "react"
import { useMutation } from "@tanstack/react-query"
import { DownloadIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import {
    downloadInquiryCustomerWorkbook,
    type InquiryCustomerWorkbookProduct,
} from "@/lib/inquiry-workbook"

type InquiryCustomerTemplateButtonProps = {
    productCount: number
    /** 下载前重新获取当前授权的商品资料和允许对客展示的价格。 */
    onLoadProducts: () => Promise<readonly InquiryCustomerWorkbookProduct[]>
    inquiry_no?: string
    id?: string
    disabled?: boolean
    size?: React.ComponentProps<typeof Button>["size"]
    variant?: React.ComponentProps<typeof Button>["variant"]
    buttonClassName?: string
}

export function InquiryCustomerTemplateButton({
    productCount,
    onLoadProducts,
    inquiry_no,
    id = "sales-inquiry-download-customer-template",
    disabled = false,
    size = "default",
    variant = "outline",
    buttonClassName,
}: InquiryCustomerTemplateButtonProps) {
    const download = useMutation({
        mutationFn: async () => {
            const currentProducts = await onLoadProducts()
            await downloadInquiryCustomerWorkbook(currentProducts, inquiry_no)
        },
    })
    return (
        <div className="space-y-2">
            <Button
                id={id}
                type="button"
                variant={variant}
                size={size}
                className={buttonClassName}
                disabled={disabled || productCount === 0 || download.isPending}
                onClick={() => download.mutate()}
            >
                <DownloadIcon data-icon="inline-start" />
                {download.isPending ? "正在生成…" : "下载客户选品模板"}
            </Button>
            {download.isError ? (
                <p role="alert" className="text-body-compact text-destructive">
                    {getErrorMessage(
                        download.error,
                        "模板下载失败，请重新获取商品后重试",
                    )}
                </p>
            ) : null}
        </div>
    )
}
