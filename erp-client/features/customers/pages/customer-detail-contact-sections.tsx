"use client"

import {
    BusinessFailureState,
    DocumentSection,
    SensitiveValue,
    surfaceInsetClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import { revealCustomerSensitiveField } from "@/features/customers/hooks/queries"
import type { CustomerCenterView } from "@/features/customers/types"

/** 联系与地址分区：联系人卡、地址卡与银行账户列表。 */
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
                title="联系与地址"
                description="有效联系人与地址；手机与履约地址按字段权限打码"
            >
                {customer.partitions.contacts === "error" ? (
                    <BusinessFailureState
                        kind="system"
                        description="联系分区失败；主体身份仍保留。"
                        action={
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => void refetch()}
                            >
                                重试分区
                            </Button>
                        }
                    />
                ) : (
                    <div className="grid gap-4 lg:grid-cols-2">
                        <Card
                            size="sm"
                            className="shadow-none ring-1 ring-foreground/[0.04]"
                        >
                            <CardHeader className="border-b border-grid">
                                <CardTitle className="text-sm">
                                    有效联系人
                                </CardTitle>
                                <CardDescription>
                                    默认打码手机；揭示操作会留记录
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3">
                                {customer.contacts.length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
                                        暂无联系人
                                    </p>
                                ) : (
                                    customer.contacts.map((c) => (
                                        <div
                                            key={c.id}
                                            className={cn(
                                                surfaceInsetClassName,
                                                "p-3 text-sm",
                                            )}
                                        >
                                            <div className="flex flex-wrap items-center gap-2">
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
                                            </div>
                                            <div className="mt-2 space-y-1 text-muted-foreground">
                                                <div className="flex flex-wrap items-center gap-2">
                                                    <span>手机</span>
                                                    {c.fieldVisibility
                                                        .phone === "masked" ? (
                                                        <SensitiveValue
                                                            label={`${c.name}手机`}
                                                            maskedValue={
                                                                c.phoneMasked
                                                            }
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
                                                </div>
                                                {c.email ? (
                                                    <div>邮箱 {c.email}</div>
                                                ) : null}
                                                <div className="text-xs">
                                                    有效期 {c.effectiveFrom}
                                                    {c.effectiveTo
                                                        ? ` ~ ${c.effectiveTo}`
                                                        : " 起"}
                                                </div>
                                            </div>
                                        </div>
                                    ))
                                )}
                            </CardContent>
                        </Card>

                        <Card
                            size="sm"
                            className="shadow-none ring-1 ring-foreground/[0.04]"
                        >
                            <CardHeader className="border-b border-grid">
                                <CardTitle className="text-sm">
                                    地址
                                </CardTitle>
                                <CardDescription>
                                    履约地址按权限打码
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3">
                                {customer.addresses.length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
                                        暂无地址
                                    </p>
                                ) : (
                                    customer.addresses.map((a) => (
                                        <div
                                            key={a.id}
                                            className={cn(
                                                surfaceInsetClassName,
                                                "p-3 text-sm",
                                            )}
                                        >
                                            <div className="flex flex-wrap items-center gap-2">
                                                <span className="font-medium">
                                                    {a.addressType}
                                                </span>
                                                {a.isDefault ? (
                                                    <Badge variant="secondary">
                                                        默认
                                                    </Badge>
                                                ) : null}
                                            </div>
                                            <div className="mt-2">
                                                {a.fieldVisibility.address ===
                                                "masked" ? (
                                                    <SensitiveValue
                                                        label={a.addressType}
                                                        maskedValue={
                                                            a.addressMasked
                                                        }
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
                                                    <span>
                                                        {a.addressMasked}
                                                    </span>
                                                )}
                                            </div>
                                        </div>
                                    ))
                                )}
                            </CardContent>
                        </Card>
                    </div>
                )}
            </DocumentSection>

            <DocumentSection
                title="银行账户"
                description="账号默认只显示末四位；完整显示需授权，操作会留记录"
            >
                {customer.bankAccounts.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        暂无银行账户
                    </p>
                ) : (
                    <Card
                        size="sm"
                        className="shadow-none ring-1 ring-foreground/[0.04]"
                    >
                        <CardContent className="space-y-2">
                            {customer.bankAccounts.map((b) => (
                                <div
                                    key={b.id}
                                    className="flex flex-wrap items-center gap-2 text-sm"
                                >
                                    <span>{b.accountName}</span>
                                    {b.isDefault ? (
                                        <Badge variant="secondary">
                                            默认
                                        </Badge>
                                    ) : null}
                                    <span className="text-muted-foreground">
                                        {b.bankName}
                                    </span>
                                    <SensitiveValue
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
                            ))}
                        </CardContent>
                    </Card>
                )}
            </DocumentSection>
        </>
    )
}
