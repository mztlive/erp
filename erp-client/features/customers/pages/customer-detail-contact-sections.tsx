"use client"

import {
    BusinessFailureState,
    DocumentSection,
    SensitiveValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { revealCustomerSensitiveField } from "@/features/customers/hooks/queries"
import type { CustomerCenterView } from "@/features/customers/types"
import {
    DetailRecordColumn,
    DetailRecordColumns,
    DetailRecordRow,
    detailSectionClassName,
    periodLabel,
} from "@/features/customers/pages/customer-detail-records"

/** 联系与地址分区：联系人、地址与银行账户按行排列。 */
export function CustomerDetailContactSections({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    return (
        <>
            <DocumentSection
                className={detailSectionClassName}
                title="联系与地址"
            >
                {customer.partitions.contacts === "error" ? (
                    <BusinessFailureState
                        kind="system"
                        description="联系分区失败；主体身份仍保留。"
                        action={
                            <Button
                                id="customers-detail-contacts-retry"
                                type="button"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重试分区
                            </Button>
                        }
                    />
                ) : (
                    <DetailRecordColumns>
                        <DetailRecordColumn label="有效联系人">
                            {customer.contacts.length === 0 ? (
                                <p className="py-2 text-sm text-muted-foreground">
                                    暂无联系人
                                </p>
                            ) : (
                                customer.contacts.map((c) => (
                                    <DetailRecordRow key={c.id}>
                                        <span className="font-medium">
                                            {c.name}
                                        </span>
                                        {c.isDefault ? (
                                            <Badge variant="secondary">
                                                默认
                                            </Badge>
                                        ) : null}
                                        {c.title ? (
                                            <span className="text-muted-foreground">
                                                {c.title}
                                            </span>
                                        ) : null}
                                        {c.fieldVisibility.phone ===
                                        "masked" ? (
                                            <SensitiveValue
                                                id={`customers-detail-contact-${toAutomationIdSegment(c.id)}-phone`}
                                                label={`${c.name}手机`}
                                                maskedValue={c.phoneMasked}
                                                onReveal={
                                                    c.phoneRevealToken
                                                        ? () =>
                                                              revealCustomerSensitiveField(
                                                                  c.phoneRevealToken!,
                                                              )
                                                        : undefined
                                                }
                                            />
                                        ) : (
                                            <span className="num">
                                                {c.phoneMasked}
                                            </span>
                                        )}
                                        {c.email ? (
                                            <span className="text-muted-foreground">
                                                {c.email}
                                            </span>
                                        ) : null}
                                        <span className="text-muted-foreground">
                                            {periodLabel(
                                                c.effectiveFrom,
                                                c.effectiveTo,
                                            )}
                                        </span>
                                    </DetailRecordRow>
                                ))
                            )}
                        </DetailRecordColumn>
                        <DetailRecordColumn label="地址">
                            {customer.addresses.length === 0 ? (
                                <p className="py-2 text-sm text-muted-foreground">
                                    暂无地址
                                </p>
                            ) : (
                                customer.addresses.map((a) => (
                                    <DetailRecordRow key={a.id}>
                                        <span className="font-medium">
                                            {a.addressType}
                                        </span>
                                        {a.isDefault ? (
                                            <Badge variant="secondary">
                                                默认
                                            </Badge>
                                        ) : null}
                                        {a.fieldVisibility.address ===
                                        "masked" ? (
                                            <SensitiveValue
                                                id={`customers-detail-address-${toAutomationIdSegment(a.id)}-address`}
                                                label={a.addressType}
                                                maskedValue={a.addressMasked}
                                                onReveal={
                                                    a.addressRevealToken
                                                        ? () =>
                                                              revealCustomerSensitiveField(
                                                                  a.addressRevealToken!,
                                                              )
                                                        : undefined
                                                }
                                            />
                                        ) : (
                                            <span>{a.addressMasked}</span>
                                        )}
                                    </DetailRecordRow>
                                ))
                            )}
                        </DetailRecordColumn>
                    </DetailRecordColumns>
                )}
            </DocumentSection>

            <DocumentSection
                className={detailSectionClassName}
                title="银行账户"
            >
                {customer.bankAccounts.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        暂无银行账户
                    </p>
                ) : (
                    <div className="divide-y divide-grid">
                        {customer.bankAccounts.map((b) => (
                            <DetailRecordRow key={b.id}>
                                <span className="font-medium">
                                    {b.accountName}
                                </span>
                                {b.isDefault ? (
                                    <Badge variant="secondary">默认</Badge>
                                ) : null}
                                <span className="text-muted-foreground">
                                    {b.bankName}
                                </span>
                                <SensitiveValue
                                    id={`customers-detail-bank-${toAutomationIdSegment(b.id)}-account`}
                                    label="银行账号"
                                    maskedValue={b.accountMasked}
                                    onReveal={
                                        b.accountRevealToken
                                            ? () =>
                                                  revealCustomerSensitiveField(
                                                      b.accountRevealToken!,
                                                  )
                                            : undefined
                                    }
                                />
                            </DetailRecordRow>
                        ))}
                    </div>
                )}
            </DocumentSection>
        </>
    )
}
