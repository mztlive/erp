"use client"

import {
    Building2Icon,
    LandmarkIcon,
    MailIcon,
    MapPinIcon,
    PhoneIcon,
    PlusIcon,
    UserRoundIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    SensitiveValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { revealCustomerSensitiveField } from "@/features/customers/hooks/queries"
import { periodLabel } from "@/features/customers/pages/customer-detail-records"
import type { CustomerCenterView } from "@/features/customers/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

function sortDefaultFirst<T extends { isDefault: boolean }>(
    items: readonly T[],
): T[] {
    return [...items].sort((a, b) => Number(b.isDefault) - Number(a.isDefault))
}

function SectionCountBadge({ count, unit }: { count: number; unit: string }) {
    if (count <= 0) return null
    return (
        <Badge variant="neutral" className="num">
            {count} {unit}
        </Badge>
    )
}

/** 联系人只读分区（结构化双行展示，完整保留手机脱敏揭示、座机与邮箱）。 */
export function CustomerReadonlyContactsSection({
    customer,
    refetch,
    onEditClick,
}: {
    customer: CustomerCenterView
    refetch: () => void
    onEditClick?: () => void
}) {
    const contacts = sortDefaultFirst(customer.contacts)

    return (
        <DetailWorkspaceSection
            title="联系人"
            badge={<SectionCountBadge count={contacts.length} unit="位" />}
            width="form"
        >
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
            ) : contacts.length === 0 ? (
                <div className="flex flex-wrap items-center gap-2 py-1 text-xs text-muted-foreground">
                    <span>暂无联系人</span>
                    {onEditClick ? (
                        <Button
                            id="customers-detail-contacts-empty-add"
                            type="button"
                            size="xs"
                            variant="ghost"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            onClick={onEditClick}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            去添加联系人
                        </Button>
                    ) : null}
                </div>
            ) : (
                <div className="divide-y divide-grid">
                    {contacts.map((c) => (
                        <div
                            key={c.id}
                            className="space-y-1.5 py-3.5 first:pt-0 last:pb-0"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div className="flex min-w-0 flex-wrap items-center gap-2">
                                    <UserRoundIcon
                                        className="size-3.5 shrink-0 text-muted-foreground"
                                        aria-hidden="true"
                                    />
                                    <span className="font-medium text-foreground">
                                        {c.name}
                                    </span>
                                    {c.title ? (
                                        <span className="text-xs text-muted-foreground">
                                            {c.title}
                                        </span>
                                    ) : null}
                                    {c.isDefault ? (
                                        <Badge variant="info">默认</Badge>
                                    ) : null}
                                </div>
                                <span className="num text-xs text-muted-foreground">
                                    {periodLabel(
                                        c.effectiveFrom,
                                        c.effectiveTo,
                                    )}
                                </span>
                            </div>
                            <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 pl-5.5 text-body-compact">
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
                                {c.telephone ? (
                                    <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                                        <PhoneIcon
                                            className="size-3 shrink-0"
                                            aria-hidden="true"
                                        />
                                        <span className="num text-foreground">
                                            {c.telephone}
                                        </span>
                                    </span>
                                ) : null}
                                {c.email ? (
                                    <span className="inline-flex min-w-0 items-center gap-1 text-xs text-muted-foreground">
                                        <MailIcon
                                            className="size-3 shrink-0"
                                            aria-hidden="true"
                                        />
                                        <span className="truncate text-foreground">
                                            {c.email}
                                        </span>
                                    </span>
                                ) : null}
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}

/** 地址只读分区（保留地址类型、联系人与脱敏地址揭示）。 */
export function CustomerReadonlyAddressesSection({
    customer,
    refetch,
    onEditClick,
}: {
    customer: CustomerCenterView
    refetch: () => void
    onEditClick?: () => void
}) {
    const addresses = sortDefaultFirst(customer.addresses)

    return (
        <DetailWorkspaceSection
            title="地址"
            badge={<SectionCountBadge count={addresses.length} unit="条" />}
            width="form"
        >
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
            ) : addresses.length === 0 ? (
                <div className="flex flex-wrap items-center gap-2 py-1 text-xs text-muted-foreground">
                    <span>暂无地址</span>
                    {onEditClick ? (
                        <Button
                            id="customers-detail-addresses-empty-add"
                            type="button"
                            size="xs"
                            variant="ghost"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            onClick={onEditClick}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            去添加地址
                        </Button>
                    ) : null}
                </div>
            ) : (
                <div className="divide-y divide-grid">
                    {addresses.map((a) => (
                        <div
                            key={a.id}
                            className="space-y-1.5 py-3.5 first:pt-0 last:pb-0"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div className="flex min-w-0 flex-wrap items-center gap-2">
                                    <MapPinIcon
                                        className="size-3.5 shrink-0 text-muted-foreground"
                                        aria-hidden="true"
                                    />
                                    <span className="font-medium text-foreground">
                                        {a.addressType}
                                    </span>
                                    {a.isDefault ? (
                                        <Badge variant="info">默认</Badge>
                                    ) : null}
                                    {a.contactName ? (
                                        <span className="text-xs text-muted-foreground">
                                            收件/对接：{a.contactName}
                                        </span>
                                    ) : null}
                                </div>
                                <span className="num text-xs text-muted-foreground">
                                    {periodLabel(
                                        a.effectiveFrom,
                                        a.effectiveTo,
                                    )}
                                </span>
                            </div>
                            <div className="pl-5.5">
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
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}

/** 银行账户只读分区（完整展示户名、开户行/支行与脱敏账号揭示）。 */
export function CustomerReadonlyBankAccountsSection({
    customer,
    onEditClick,
}: {
    customer: CustomerCenterView
    onEditClick?: () => void
}) {
    const bankAccounts = sortDefaultFirst(customer.bankAccounts)

    return (
        <DetailWorkspaceSection
            title="银行账户"
            badge={<SectionCountBadge count={bankAccounts.length} unit="个" />}
            width="form"
        >
            {bankAccounts.length === 0 ? (
                <div className="flex flex-wrap items-center gap-2 py-1 text-xs text-muted-foreground">
                    <span>暂无银行账户</span>
                    {onEditClick ? (
                        <Button
                            id="customers-detail-banks-empty-add"
                            type="button"
                            size="xs"
                            variant="ghost"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            onClick={onEditClick}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            去添加账户
                        </Button>
                    ) : null}
                </div>
            ) : (
                <div className="divide-y divide-grid">
                    {bankAccounts.map((b) => (
                        <div
                            key={b.id}
                            className="space-y-1.5 py-3.5 first:pt-0 last:pb-0"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div className="flex min-w-0 flex-wrap items-center gap-2">
                                    <LandmarkIcon
                                        className="size-3.5 shrink-0 text-muted-foreground"
                                        aria-hidden="true"
                                    />
                                    <span className="font-medium text-foreground">
                                        {b.accountName}
                                    </span>
                                    {b.isDefault ? (
                                        <Badge variant="info">默认</Badge>
                                    ) : null}
                                </div>
                                <span className="num text-xs text-muted-foreground">
                                    {periodLabel(
                                        b.effectiveFrom,
                                        b.effectiveTo,
                                    )}
                                </span>
                            </div>
                            <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 pl-5.5">
                                <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                                    <Building2Icon
                                        className="size-3 shrink-0"
                                        aria-hidden="true"
                                    />
                                    <span>
                                        {[b.bankName, b.branchName]
                                            .filter(Boolean)
                                            .join(" · ")}
                                    </span>
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
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </DetailWorkspaceSection>
    )
}

/** 客户概览浏览态下的联系人、地址与银行账户分组，严格按字段查看权限显隐。 */
export function CustomerDetailContactSections({
    customer,
    refetch,
    onEditClick,
}: {
    customer: CustomerCenterView
    refetch: () => void
    onEditClick?: () => void
}) {
    const accountProfile = useAccountProfileQuery()
    const permissions = accountProfile.data?.permissions
    const canReadContacts = accountProfile.isSuccess
        ? hasPermission(permissions, "party_contact:detail")
        : customer.contacts.length > 0
    const canReadAddresses = accountProfile.isSuccess
        ? hasPermission(permissions, "party_address:detail")
        : customer.addresses.length > 0
    const canReadBanks = accountProfile.isSuccess
        ? hasPermission(permissions, "party_bank_account:detail")
        : customer.bankAccounts.length > 0

    const canWriteContacts =
        Boolean(onEditClick) &&
        hasPermission(permissions, "party_contact:update")
    const canWriteAddresses =
        Boolean(onEditClick) &&
        hasPermission(permissions, "party_address:update")
    const canWriteBanks =
        Boolean(onEditClick) &&
        hasPermission(permissions, "party_bank_account:update")

    return (
        <>
            {canReadContacts ? (
                <CustomerReadonlyContactsSection
                    customer={customer}
                    refetch={refetch}
                    onEditClick={canWriteContacts ? onEditClick : undefined}
                />
            ) : null}
            {canReadAddresses ? (
                <CustomerReadonlyAddressesSection
                    customer={customer}
                    refetch={refetch}
                    onEditClick={canWriteAddresses ? onEditClick : undefined}
                />
            ) : null}
            {canReadBanks ? (
                <CustomerReadonlyBankAccountsSection
                    customer={customer}
                    onEditClick={canWriteBanks ? onEditClick : undefined}
                />
            ) : null}
        </>
    )
}
