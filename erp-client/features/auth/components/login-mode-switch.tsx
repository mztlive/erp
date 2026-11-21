"use client"

import Link from "next/link"

import { cn } from "@/lib/utils"

const modes = [
    { id: "admin", label: "内部登录", href: "/login" },
    {
        id: "supplier",
        label: "供应商登录",
        href: "/supplier-portal/login",
    },
] as const

export function LoginModeSwitch({ mode }: { mode: "admin" | "supplier" }) {
    return (
        <nav
            aria-label="登录身份"
            className="grid grid-cols-2 gap-1 rounded-lg bg-muted p-1"
        >
            {modes.map((item) => (
                <Link
                    key={item.id}
                    id={`login-mode-${item.id}`}
                    href={item.href}
                    aria-current={mode === item.id ? "page" : undefined}
                    className={cn(
                        "rounded-md px-4 py-2 text-center text-body-compact font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
                        mode === item.id
                            ? "bg-background text-foreground shadow-sm"
                            : "text-muted-foreground hover:text-foreground",
                    )}
                >
                    {item.label}
                </Link>
            ))}
        </nav>
    )
}
