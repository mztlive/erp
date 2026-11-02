"use client"

import type { ReactNode } from "react"

import { LoginStage } from "./login-stage"

export function LoginLayout({
    mode,
    children,
}: {
    mode: "admin" | "supplier"
    children: ReactNode
}) {
    return (
        <div className="grid min-h-svh bg-background lg:grid-cols-[minmax(0,1.15fr)_minmax(26rem,0.85fr)]">
            <LoginStage mode={mode} />
            <main className="relative flex items-center justify-center bg-card p-6 sm:p-10">
                {children}
            </main>
        </div>
    )
}
