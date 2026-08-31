"use client"

import { SensitiveValue, surfacePanelClassName } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { useRevealPaymentRecipientMutation } from "@/features/supplier-payables/hooks/queries"
import type { PaymentRecipient } from "@/features/supplier-payables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type PaymentRecipientRevealProps = Readonly<{
    payableAccountId: string
    workItemId: string
    expectedTaskVersion: string
    recipient: PaymentRecipient
}>

function bankLabelOf(recipient: PaymentRecipient) {
    return (
        [recipient.bankName, recipient.bankBranchName]
            .filter(Boolean)
            .join(" · ") || "未填写"
    )
}

/** 收款账户三要素；账号明文仅在当前任务责任校验后短时揭示。 */
export function PaymentRecipientFields({
    payableAccountId,
    workItemId,
    expectedTaskVersion,
    recipient,
}: PaymentRecipientRevealProps) {
    const reveal = useRevealPaymentRecipientMutation()

    return (
        <DescriptionList columns="three">
            <DescriptionItem>
                <DescriptionTerm>收款户名</DescriptionTerm>
                <DescriptionDetails>{recipient.accountName}</DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
                <DescriptionTerm>开户行</DescriptionTerm>
                <DescriptionDetails>
                    {bankLabelOf(recipient)}
                </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
                <DescriptionTerm>收款账号</DescriptionTerm>
                <DescriptionDetails>
                    <SensitiveValue
                        id={`supplier-payables-payment-recipient-${toAutomationIdSegment(payableAccountId)}-account`}
                        label="收款账号"
                        maskedValue={recipient.accountNumberMasked}
                        onReveal={async () => {
                            try {
                                return await reveal.mutateAsync({
                                    payableAccountId,
                                    workItemId,
                                    expectedTaskVersion,
                                    expectedBankAccountId:
                                        recipient.bankAccountId,
                                    expectedBankAccountVersion:
                                        recipient.version,
                                })
                            } finally {
                                reveal.reset()
                            }
                        }}
                    />
                </DescriptionDetails>
            </DescriptionItem>
        </DescriptionList>
    )
}

export function PaymentRecipientHeading({
    className,
    payableAccountId,
}: {
    className?: string
    payableAccountId?: string
}) {
    const titleId = payableAccountId
        ? `supplier-payables-payment-recipient-${toAutomationIdSegment(payableAccountId)}-title`
        : "supplier-payables-payment-recipient-title"
    return (
        <div className={className}>
            <h3 id={titleId} className="text-sm font-semibold">
                收款信息
            </h3>
            <p className="mt-1 text-xs text-muted-foreground">
                付款前请与采购合同或供应商资料复核；完整账号仅短时显示并记录审计。
            </p>
        </div>
    )
}

/** 展示付款所需的收款账户摘要；账号明文仅在当前任务责任校验后短时揭示。 */
export function PaymentRecipientCard({
    payableAccountId,
    workItemId,
    expectedTaskVersion,
    recipient,
}: PaymentRecipientRevealProps) {
    const titleId = `supplier-payables-payment-recipient-${toAutomationIdSegment(payableAccountId)}-title`
    return (
        <section
            className={cn(surfacePanelClassName, "space-y-4 p-4")}
            aria-labelledby={titleId}
        >
            <PaymentRecipientHeading payableAccountId={payableAccountId} />
            <PaymentRecipientFields
                payableAccountId={payableAccountId}
                workItemId={workItemId}
                expectedTaskVersion={expectedTaskVersion}
                recipient={recipient}
            />
        </section>
    )
}
