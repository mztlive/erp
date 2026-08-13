"use client"

import { surfacePanelClassName } from "@/components/business"
import { cn } from "@/lib/utils"

export function WhitelistContentGrid({
    content,
    revisionNo,
}: {
    content: {
        customerExternalIdentity: string
        customerExternalIdentityCopyable: boolean
        voucherCategoryExternalIdentity: string
        voucherCategoryErpName: string
        voucherExpiryAt: string
        faceValue: string
        cardCount: string
        cardForm: string
        effectiveAt: string
        contentHash: string
    }
    /** 数据修订号（用户可见的版本，不展示内容哈希） */
    revisionNo?: number
}) {
    return (
        <dl
            className={cn(
                "grid gap-3 sm:grid-cols-2 p-3 text-sm",
                surfacePanelClassName,
            )}
        >
            <div>
                <dt className="text-xs text-muted-foreground">商城客户引用</dt>
                <dd className="num font-medium">
                    {content.customerExternalIdentity}
                    {!content.customerExternalIdentityCopyable ? (
                        <span className="ml-2 text-xs font-normal text-muted-foreground">
                            仅显示引用摘要，不可复制完整值
                        </span>
                    ) : null}
                </dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">商城卡券类目</dt>
                <dd>
                    {content.voucherCategoryErpName}
                    <span className="ml-2 num text-xs text-muted-foreground">
                        {content.voucherCategoryExternalIdentity}
                    </span>
                </dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">履约期限</dt>
                <dd className="num">{content.voucherExpiryAt}</dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">面额</dt>
                <dd className="num">{content.faceValue}</dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">数量</dt>
                <dd className="num">{content.cardCount}</dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">卡形态</dt>
                <dd>{content.cardForm}</dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">ERP 生效时间</dt>
                <dd className="num">{content.effectiveAt}</dd>
            </div>
            <div>
                <dt className="text-xs text-muted-foreground">数据版本</dt>
                <dd className="num text-xs">
                    {revisionNo != null ? `v${revisionNo}` : "—"}
                </dd>
            </div>
        </dl>
    )
}
