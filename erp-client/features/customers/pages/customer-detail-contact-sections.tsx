"use client"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
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
    periodLabel,
} from "@/features/customers/pages/customer-detail-records"

/** 联系与地址只读分区（统一 width="form"）。 */
export function CustomerReadonlyContactsSection({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    return (
        <DetailWorkspaceSection title="联系人" width="form">
            {customer.partitions.contacts === "error" ? (
                <BusinessFailureState
                    kind="system"
                    description="联系人加载失败，请重试"
                    action={
                        <Button
                            id="customers-detail-contacts-retry"
                            type="button"
                            size="sm"
                            onClick={() => void refetch()}
                        >
                            重新加载
                        </Button>
                    }
                />
            ) : customer.contacts.length === 0 ? (
                <p className="text-xs text-muted-foreground">暂无联系人</p>
            ) : (
                <div className="divide-y divide-grid">
                    {customer.contacts.map((c) => (
                        <DetailRecordRow key={c.id}>
                            <span className="font-medium">{c.name}</span>
                            {c.isDefault ? (
                                <Badge variant="secondary">默认</Badge>
                            ) : null}
                            {c.title ? (
                                <span className="text-muted-foreground">
                                    {c.title}
                                </span>
                            ) : null}
                            {c.fieldVisibility.phone === "masked" ? (
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
                                <span className="num">{c.phoneMasked}</span>
                            )}
                            {c.email ? (
                                <span className="text-muted-foreground">
                                    {c.email}
                                </span>
                            ) : null}
                            <span className="num text-muted-foreground">
                                {periodLabel(c.effectiveFrom, c.effectiveTo)}
                            </span>
                        </DetailRecordRow>
                    ))}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}

export function CustomerReadonlyAddressesSection({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    return (
        <DetailWorkspaceSection title="地址" width="form">
            {customer.partitions.contacts === "error" ? (
                <BusinessFailureState
                    kind="system"
                    description="地址加载失败，请重试"
                    action={
                        <Button
                            id="customers-detail-addresses-retry"
                            type="button"
                            size="sm"
                            onClick={() => void refetch()}
                        >
                            重新加载
                        </Button>
                    }
                />
            ) : customer.addresses.length === 0 ? (
                <p className="text-xs text-muted-foreground">暂无地址</p>
            ) : (
                <div className="divide-y divide-grid">
                    {customer.addresses.map((a) => (
                        <DetailRecordRow key={a.id}>
                            <span className="font-medium">{a.addressType}</span>
                            {a.isDefault ? (
                                <Badge variant="secondary">默认</Badge>
                            ) : null}
                            {a.fieldVisibility.address === "masked" ? (
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
                    ))}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}

export function CustomerReadonlyBankAccountsSection({
    customer,
}: {
    customer: CustomerCenterView
}) {
    return (
        <DetailWorkspaceSection title="银行账户" width="form">
            {customer.bankAccounts.length === 0 ? (
                <p className="text-xs text-muted-foreground">暂无银行账户</p>
            ) : (
                <div className="divide-y divide-grid">
                    {customer.bankAccounts.map((b) => (
                        <DetailRecordRow key={b.id}>
                            <span className="font-medium">{b.accountName}</span>
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
        </DetailWorkspaceSection>
    )
}

/** 联系与地址分区（只读角色整块展示，统一 width="form" 对齐基本资料区）。 */
export function CustomerDetailContactSections({
    customer,
    refetch,
}: {
    customer: CustomerCenterView
    refetch: () => void
}) {
    return (
        <>
            <DetailWorkspaceSection title="联系与地址" width="form">
                {customer.partitions.contacts === "error" ? (
                    <BusinessFailureState
                        kind="system"
                        description="联系方式与地址加载失败，请重试"
                        action={
                            <Button
                                id="customers-detail-contacts-retry"
                                type="button"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重新加载
                            </Button>
                        }
                    />
                ) : (
                    <DetailRecordColumns>
                        <DetailRecordColumn label="有效联系人">
                            {customer.contacts.length === 0 ? (
                                <p className="py-2 text-xs text-muted-foreground">
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
                                        <span className="num text-muted-foreground">
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
                                <p className="py-2 text-xs text-muted-foreground">
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
            </DetailWorkspaceSection>

            <CustomerReadonlyBankAccountsSection customer={customer} />
        </>
    )
}
